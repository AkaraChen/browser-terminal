use std::{
    env,
    io::{self, Read, Write},
    net::SocketAddr,
    sync::mpsc as std_mpsc,
    thread,
};

use anyhow::{Context, Result};
use axum::{
    Router,
    extract::{
        Path, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::header,
    response::{Html, IntoResponse, Response},
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

const INDEX_HTML: &str = include_str!("../static/index.html");

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "browser_terminal=info,tower_http=info".into()),
        )
        .init();

    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3000);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let app = Router::new()
        .route("/", get(index))
        .route("/ws/{channel}", get(ws_handler))
        .route("/healthz", get(|| async { "ok" }));

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;
    let local_addr = listener.local_addr()?;

    println!("browser-terminal listening on http://{local_addr}");
    info!(%local_addr, "server started");

    axum::serve(listener, app).await.context("server failed")
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
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let mut command = CommandBuilder::new(shell);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        if let Some(home) = dirs::home_dir() {
            command.cwd(home);
        }

        let child = pair
            .slave
            .spawn_command(command)
            .context("failed to spawn shell")?;
        drop(pair.slave);

        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

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
