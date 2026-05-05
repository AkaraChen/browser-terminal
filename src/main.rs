use std::{
    env, fs,
    io::{self, Read, Write},
    net::IpAddr,
    path::PathBuf,
    sync::mpsc as std_mpsc,
    thread,
};

use anyhow::{Context, Result};
use axum::{
    Router,
    extract::{
        Path, Request, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header, uri::Authority},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use base64::{Engine as _, engine::general_purpose};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use rand::distr::{Alphanumeric, SampleString};
use serde::Deserialize;
use tokio::sync::mpsc;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::{debug, error, info};

const INDEX_HTML: &str = include_str!("../static/index.html");
const CONFIG_FILE_NAME: &str = ".browser-terminalrc";
const DEFAULT_USERNAME: &str = "admin";
const GENERATED_PASSWORD_LEN: usize = 24;

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    /// Host name or address to bind the server to.
    #[arg(long, env = "HOST", default_value = "127.0.0.1")]
    host: String,

    /// Port to bind the server to.
    #[arg(long, env = "PORT", default_value_t = 3000)]
    port: u16,

    /// CORS origin to allow, for example http://localhost:5173.
    ///
    /// When omitted, only loopback origins on this server's port are allowed.
    #[arg(long, env = "CORS_ORIGIN", value_parser = parse_cors_origin)]
    cors_origin: Option<HeaderValue>,

    /// Allow arbitrary Host and Origin headers.
    ///
    /// This broadens DNS rebinding exposure and should only be used when you
    /// understand that Basic Auth is the remaining protection.
    #[arg(long, env = "DANGEROUS_ALLOW_ALL_HOST", default_value_t = false)]
    dangerous_allow_all_host: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "browser_terminal=info,tower_http=info".into()),
        )
        .init();

    let args = Args::parse();
    let auth = BasicAuth::load().context("failed to load Basic Auth configuration")?;

    let listener = tokio::net::TcpListener::bind((args.host.as_str(), args.port))
        .await
        .with_context(|| format!("failed to bind {}:{}", args.host, args.port))?;
    let local_addr = listener.local_addr()?;

    let security = SecurityPolicy::new(
        args.cors_origin.clone(),
        args.dangerous_allow_all_host,
        local_addr.port(),
    );

    let app = Router::new()
        .route("/", get(index))
        .route("/ws/{channel}", get(ws_handler))
        .route("/healthz", get(|| async { "ok" }))
        .layer(middleware::from_fn_with_state(
            AppState {
                auth: auth.clone(),
                security: security.clone(),
            },
            security_middleware,
        ))
        .layer(cors_layer(security.clone()));

    println!("browser-terminal listening on http://{local_addr}");
    println!("basic auth username: {}", auth.username);
    println!("basic auth password: {}", auth.password);
    println!("basic auth password source: {}", auth.password_source);
    println!("allowed host/origin policy: {}", security.description());
    info!(%local_addr, "server started");

    axum::serve(listener, app).await.context("server failed")
}

fn cors_layer(security: SecurityPolicy) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(security.allow_origin())
        .allow_methods([Method::GET])
        .allow_headers([header::AUTHORIZATION])
        .allow_credentials(true)
}

fn parse_cors_origin(value: &str) -> std::result::Result<HeaderValue, String> {
    let origin =
        HeaderValue::from_str(value).map_err(|err| format!("invalid header value: {err}"))?;
    validate_origin(&origin)?;
    Ok(origin)
}

fn validate_origin(origin: &HeaderValue) -> std::result::Result<(), String> {
    let origin = origin
        .to_str()
        .map_err(|_| "origin must contain visible ASCII characters".to_string())?;
    let uri = origin.parse::<Uri>().map_err(|_| {
        "origin must be a valid URL origin, for example http://localhost:5173".to_string()
    })?;

    match uri.scheme_str() {
        Some("http") | Some("https") => {}
        _ => return Err("origin scheme must be http or https".to_string()),
    }

    if uri.authority().is_none() {
        return Err("origin must include a host".to_string());
    }

    if uri.path() != "/" || uri.query().is_some() {
        return Err("origin must not include a path or query".to_string());
    }

    Ok(())
}

#[derive(Clone, Debug)]
struct AppState {
    auth: BasicAuth,
    security: SecurityPolicy,
}

#[derive(Clone, Debug)]
struct BasicAuth {
    username: String,
    password: String,
    password_source: PasswordSource,
}

#[derive(Clone, Debug)]
enum PasswordSource {
    Generated,
    Config(PathBuf),
}

impl std::fmt::Display for PasswordSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Generated => formatter.write_str("generated"),
            Self::Config(path) => write!(formatter, "{}", path.display()),
        }
    }
}

impl BasicAuth {
    fn load() -> Result<Self> {
        let rc = ConfigFile::load()?;
        let username = rc
            .as_ref()
            .and_then(|config| config.username.clone())
            .unwrap_or_else(|| DEFAULT_USERNAME.to_string());

        let (password, password_source) = if let Some(config) = rc {
            if let Some(password) = config.password {
                (password, PasswordSource::Config(config.path))
            } else {
                (generate_password(), PasswordSource::Generated)
            }
        } else {
            (generate_password(), PasswordSource::Generated)
        };

        Ok(Self {
            username,
            password,
            password_source,
        })
    }

    fn allows_headers(&self, headers: &HeaderMap) -> bool {
        let Some(value) = headers.get(header::AUTHORIZATION) else {
            return false;
        };
        let Ok(value) = value.to_str() else {
            return false;
        };
        let Some(encoded) = value.strip_prefix("Basic ") else {
            return false;
        };
        let Ok(decoded) = general_purpose::STANDARD.decode(encoded) else {
            return false;
        };
        let Ok(decoded) = String::from_utf8(decoded) else {
            return false;
        };
        let Some((username, password)) = decoded.split_once(':') else {
            return false;
        };

        username == self.username && password == self.password
    }
}

#[derive(Debug)]
struct ConfigFile {
    path: PathBuf,
    username: Option<String>,
    password: Option<String>,
}

impl ConfigFile {
    fn load() -> Result<Option<Self>> {
        let Some(path) = config_path() else {
            return Ok(None);
        };

        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let mut config = parse_config_file(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        config.path = path;
        Ok(Some(config))
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(CONFIG_FILE_NAME))
}

fn parse_config_file(content: &str) -> Result<ConfigFile> {
    let mut config = ConfigFile {
        path: PathBuf::new(),
        username: None,
        password: None,
    };

    for (index, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = split_key_value(line) else {
            anyhow::bail!("line {} must use key=value or key: value", index + 1);
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();

        if value.is_empty() {
            anyhow::bail!("line {} has an empty value for {key}", index + 1);
        }

        match key.as_str() {
            "username" | "user" => config.username = Some(value.to_string()),
            "password" | "basic_auth_password" => config.password = Some(value.to_string()),
            _ => debug!(key, line = index + 1, "ignoring unknown config key"),
        }
    }

    Ok(config)
}

fn split_key_value(line: &str) -> Option<(&str, &str)> {
    match (line.find('='), line.find(':')) {
        (Some(equal), Some(colon)) if equal < colon => Some(line.split_at(equal)),
        (Some(_equal), Some(colon)) => Some(line.split_at(colon)),
        (Some(equal), None) => Some(line.split_at(equal)),
        (None, Some(colon)) => Some(line.split_at(colon)),
        (None, None) => None,
    }
    .map(|(key, value)| (key, &value[1..]))
}

fn generate_password() -> String {
    Alphanumeric.sample_string(&mut rand::rng(), GENERATED_PASSWORD_LEN)
}

#[derive(Clone, Debug)]
struct SecurityPolicy {
    origin_policy: OriginPolicy,
    host_policy: HostPolicy,
}

impl SecurityPolicy {
    fn new(
        cors_origin: Option<HeaderValue>,
        dangerous_allow_all_host: bool,
        server_port: u16,
    ) -> Self {
        if dangerous_allow_all_host {
            Self {
                origin_policy: OriginPolicy::AllowAll,
                host_policy: HostPolicy::AllowAll,
            }
        } else if let Some(origin) = cors_origin {
            Self {
                origin_policy: OriginPolicy::Exact(origin),
                host_policy: HostPolicy::Loopback { server_port },
            }
        } else {
            Self {
                origin_policy: OriginPolicy::Loopback { server_port },
                host_policy: HostPolicy::Loopback { server_port },
            }
        }
    }

    fn allow_origin(&self) -> AllowOrigin {
        self.origin_policy.allow_origin()
    }

    fn allows_headers(&self, headers: &HeaderMap) -> bool {
        self.host_policy.allows_headers(headers) && self.origin_policy.allows_headers(headers)
    }

    fn description(&self) -> String {
        match (&self.host_policy, &self.origin_policy) {
            (HostPolicy::AllowAll, OriginPolicy::AllowAll) => {
                "dangerously allowing all Host and Origin headers".to_string()
            }
            (HostPolicy::Loopback { server_port }, OriginPolicy::Exact(origin)) => {
                let origin = origin.to_str().unwrap_or("<invalid utf8 origin>");
                format!("loopback Host on port {server_port}; exact Origin {origin}")
            }
            (HostPolicy::Loopback { server_port }, OriginPolicy::Loopback { .. }) => {
                format!("loopback Host and Origin on port {server_port}")
            }
            _ => "custom Host and Origin policy".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
enum HostPolicy {
    Loopback { server_port: u16 },
    AllowAll,
}

impl HostPolicy {
    fn allows_headers(&self, headers: &HeaderMap) -> bool {
        match self {
            Self::AllowAll => true,
            Self::Loopback { server_port } => headers
                .get(header::HOST)
                .and_then(|host| host.to_str().ok())
                .and_then(parse_authority)
                .is_some_and(|authority| authority_is_loopback_on_port(&authority, *server_port)),
        }
    }
}

#[derive(Clone, Debug)]
enum OriginPolicy {
    Exact(HeaderValue),
    Loopback { server_port: u16 },
    AllowAll,
}

impl OriginPolicy {
    fn allow_origin(&self) -> AllowOrigin {
        match self {
            Self::Exact(origin) => AllowOrigin::exact(origin.clone()),
            Self::Loopback { server_port } => {
                let server_port = *server_port;
                AllowOrigin::predicate(move |origin, _request_parts| {
                    origin_is_loopback_on_port(origin, server_port)
                })
            }
            Self::AllowAll => AllowOrigin::predicate(|_origin, _request_parts| true),
        }
    }

    fn allows_headers(&self, headers: &HeaderMap) -> bool {
        let Some(origin) = headers.get(header::ORIGIN) else {
            return true;
        };

        match self {
            Self::Exact(allowed_origin) => origin == allowed_origin,
            Self::Loopback { server_port } => origin_is_loopback_on_port(origin, *server_port),
            Self::AllowAll => true,
        }
    }
}

fn parse_authority(value: &str) -> Option<Authority> {
    value.parse::<Authority>().ok()
}

fn origin_is_loopback_on_port(origin: &HeaderValue, server_port: u16) -> bool {
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Ok(origin) = origin.parse::<Uri>() else {
        return false;
    };

    if !matches!(origin.scheme_str(), Some("http" | "https")) {
        return false;
    }

    let Some(authority) = origin.authority() else {
        return false;
    };

    authority_is_loopback_on_port(authority, server_port)
}

fn authority_is_loopback_on_port(authority: &Authority, server_port: u16) -> bool {
    authority_port(authority) == Some(server_port) && authority_host_is_loopback(authority)
}

fn authority_port(authority: &Authority) -> Option<u16> {
    authority.port_u16().or(Some(80))
}

fn authority_host_is_loopback(authority: &Authority) -> bool {
    let host = authority.host().trim_matches(['[', ']']);

    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

async fn security_middleware(
    axum::extract::State(state): axum::extract::State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if !state.security.allows_headers(request.headers()) {
        return (StatusCode::FORBIDDEN, "forbidden host or origin").into_response();
    }

    if !state.auth.allows_headers(request.headers()) {
        return basic_auth_challenge();
    }

    next.run(request).await
}

fn basic_auth_challenge() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Basic realm=\"Browser Terminal\", charset=\"UTF-8\""),
        )],
        "authentication required",
    )
        .into_response()
}

async fn index() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        Html(INDEX_HTML),
    )
}

async fn ws_handler(ws: WebSocketUpgrade, Path(channel): Path<String>) -> Response {
    ws.on_upgrade(move |socket| async move {
        if let Err(err) = handle_socket(socket, channel).await {
            error!(error = %err, "terminal session failed");
        }
    })
}

async fn handle_socket(socket: WebSocket, channel: String) -> Result<()> {
    info!(%channel, "opening terminal session");

    let pty = PtyProcess::spawn().context("failed to spawn pty")?;
    let mut pty_control = pty.control;
    let (pty_output_tx, mut pty_output_rx) = mpsc::channel::<Vec<u8>>(256);
    let (pty_input_tx, pty_input_rx) = std_mpsc::channel::<Vec<u8>>();

    spawn_pty_reader(pty.reader, pty_output_tx);
    spawn_pty_writer(pty.writer, pty_input_rx);

    let (mut ws_sender, mut ws_receiver) = socket.split();

    loop {
        tokio::select! {
            maybe_output = pty_output_rx.recv() => {
                let Some(output) = maybe_output else {
                    debug!(%channel, "pty output channel closed");
                    break;
                };

                if ws_sender.send(Message::Binary(output.into())).await.is_err() {
                    debug!(%channel, "websocket sender closed");
                    break;
                }
            }
            maybe_message = ws_receiver.next() => {
                let Some(message) = maybe_message else {
                    debug!(%channel, "websocket receiver closed");
                    break;
                };

                match message {
                    Ok(Message::Binary(bytes)) => {
                        if pty_input_tx.send(bytes.to_vec()).is_err() {
                            break;
                        }
                    }
                    Ok(Message::Text(text)) => {
                        if let Some(resize) = parse_resize_message(&text)? {
                            pty_control.resize(resize)?;
                        } else if pty_input_tx.send(text.as_bytes().to_vec()).is_err() {
                            break;
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {}
                    Err(err) => {
                        debug!(%channel, error = %err, "websocket error");
                        break;
                    }
                }
            }
        }
    }

    drop(pty_input_tx);
    if let Err(err) = pty_control.child.kill() {
        debug!(%channel, error = %err, "failed to kill child process");
    }
    info!(%channel, "terminal session closed");

    Ok(())
}

fn parse_resize_message(text: &str) -> Result<Option<PtySize>> {
    let Ok(message) = serde_json::from_str::<ClientMessage>(text) else {
        return Ok(None);
    };

    match message {
        ClientMessage::Resize { cols, rows } => {
            let cols = cols.clamp(2, 512);
            let rows = rows.clamp(2, 512);
            Ok(Some(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            }))
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    #[serde(rename = "resize")]
    Resize { cols: u16, rows: u16 },
}

struct PtyProcess {
    control: PtyControl,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
}

struct PtyControl {
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl PtyProcess {
    fn spawn() -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to open pty")?;

        let shell = if cfg!(windows) {
            env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
        } else {
            env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
        };
        let mut command = CommandBuilder::new(&shell);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        if let Some(home) = dirs::home_dir() {
            command.cwd(&home);
        }

        info!(%shell, "spawning pty shell");

        let child = pair
            .slave
            .spawn_command(command)
            .context("failed to spawn shell")?;
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .context("failed to clone pty reader")?;
        let writer = pair
            .master
            .take_writer()
            .context("failed to take pty writer")?;

        Ok(Self {
            control: PtyControl {
                master: pair.master,
                child,
            },
            reader,
            writer,
        })
    }
}

impl PtyControl {
    fn resize(&mut self, size: PtySize) -> Result<()> {
        self.master.resize(size).context("failed to resize pty")
    }
}

fn spawn_pty_reader(mut reader: Box<dyn Read + Send>, output_tx: mpsc::Sender<Vec<u8>>) {
    thread::spawn(move || {
        let mut buf = [0_u8; 8192];

        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if output_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
    });
}

fn spawn_pty_writer(mut writer: Box<dyn Write + Send>, input_rx: std_mpsc::Receiver<Vec<u8>>) {
    thread::spawn(move || {
        while let Ok(input) = input_rx.recv() {
            if writer.write_all(&input).is_err() {
                break;
            }
            let _ = writer.flush();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_allows_loopback_host_and_origin_on_server_port() {
        let policy = SecurityPolicy::new(None, false, 3000);
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("localhost:3000"));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://127.0.0.1:3000"),
        );

        assert!(policy.allows_headers(&headers));
    }

    #[test]
    fn default_policy_rejects_rebound_host_and_origin() {
        let policy = SecurityPolicy::new(None, false, 3000);
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("evil.example:3000"));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://evil.example:3000"),
        );

        assert!(!policy.allows_headers(&headers));
    }

    #[test]
    fn default_policy_rejects_loopback_origin_on_different_port() {
        let policy = SecurityPolicy::new(None, false, 3000);
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("localhost:3000"));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://localhost:5173"),
        );

        assert!(!policy.allows_headers(&headers));
    }

    #[test]
    fn explicit_origin_allows_frontend_origin_with_loopback_host() {
        let policy = SecurityPolicy::new(
            Some(parse_cors_origin("http://localhost:5173").unwrap()),
            false,
            3000,
        );
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:3000"));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://localhost:5173"),
        );

        assert!(policy.allows_headers(&headers));
    }

    #[test]
    fn dangerous_policy_allows_rebound_headers() {
        let policy = SecurityPolicy::new(None, true, 3000);
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("evil.example:3000"));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://evil.example:3000"),
        );

        assert!(policy.allows_headers(&headers));
    }

    #[test]
    fn configured_cors_origin_must_not_include_path() {
        let err = parse_cors_origin("http://localhost:5173/app").unwrap_err();

        assert!(err.contains("path"));
    }

    #[test]
    fn basic_auth_accepts_expected_credentials() {
        let auth = BasicAuth {
            username: "admin".to_string(),
            password: "secret".to_string(),
            password_source: PasswordSource::Generated,
        };
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, basic_auth_header("admin", "secret"));

        assert!(auth.allows_headers(&headers));
    }

    #[test]
    fn basic_auth_rejects_missing_credentials() {
        let auth = BasicAuth {
            username: "admin".to_string(),
            password: "secret".to_string(),
            password_source: PasswordSource::Generated,
        };

        assert!(!auth.allows_headers(&HeaderMap::new()));
    }

    #[test]
    fn config_file_parses_key_value_auth_settings() {
        let config = parse_config_file(
            r#"
            # Browser Terminal
            username = admin
            password: fixed-password
            "#,
        )
        .unwrap();

        assert_eq!(config.username.as_deref(), Some("admin"));
        assert_eq!(config.password.as_deref(), Some("fixed-password"));
    }

    fn basic_auth_header(username: &str, password: &str) -> HeaderValue {
        let encoded = general_purpose::STANDARD.encode(format!("{username}:{password}"));
        HeaderValue::from_str(&format!("Basic {encoded}")).unwrap()
    }
}
