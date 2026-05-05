mod auth;
mod cli;
mod guard;
mod pty;
mod security;
mod static_assets;
mod ws;

use anyhow::{Context, Result};
use axum::{Router, http::HeaderValue, middleware, routing::get};
use clap::Parser;
use std::net::IpAddr;
use tracing::info;

use crate::{
    auth::BasicAuth,
    cli::Args,
    guard::{AppState, security_middleware},
    security::{SecurityPolicy, cors_layer, origin_host_is_loopback},
    static_assets::static_handler,
    ws::ws_handler,
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "browser_terminal=info,tower_http=info".into()),
        )
        .init();

    let args = Args::parse();

    let listener = tokio::net::TcpListener::bind((args.host.as_str(), args.port))
        .await
        .with_context(|| format!("failed to bind {}:{}", args.host, args.port))?;
    let local_addr = listener.local_addr()?;
    let auth = BasicAuth::load(default_requires_auth(
        local_addr.ip(),
        args.cors_origin.as_ref(),
        args.dangerous_allow_all_host,
    ))
    .context("failed to load Basic Auth configuration")?;

    let security = SecurityPolicy::new(
        args.cors_origin.clone(),
        args.dangerous_allow_all_host,
        local_addr.port(),
    );

    let app = Router::new()
        .route("/ws/{channel}", get(ws_handler))
        .route("/healthz", get(|| async { "ok" }))
        .fallback(static_handler)
        .layer(middleware::from_fn_with_state(
            AppState {
                auth: auth.clone(),
                security: security.clone(),
            },
            security_middleware,
        ))
        .layer(cors_layer(security.clone()));

    println!("browser-terminal listening on http://{local_addr}");
    if let Some(credentials) = auth.credentials() {
        println!("basic auth username: {}", credentials.username);
        println!("basic auth password: {}", credentials.password);
        println!(
            "basic auth password source: {}",
            credentials.password_source
        );
    } else {
        println!("basic auth: disabled for loopback listener");
    }
    println!("allowed host/origin policy: {}", security.description());
    info!(%local_addr, "server started");

    axum::serve(listener, app).await.context("server failed")
}

fn default_requires_auth(
    listener_ip: IpAddr,
    cors_origin: Option<&HeaderValue>,
    dangerous_allow_all_host: bool,
) -> bool {
    dangerous_allow_all_host
        || !listener_ip.is_loopback()
        || cors_origin.is_some_and(|origin| !origin_host_is_loopback(origin))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_listener_does_not_require_auth_by_default() {
        assert!(!default_requires_auth(
            "127.0.0.1".parse().unwrap(),
            None,
            false
        ));
    }

    #[test]
    fn loopback_cors_origin_does_not_require_auth_by_default() {
        let origin = HeaderValue::from_static("http://localhost:5173");

        assert!(!default_requires_auth(
            "127.0.0.1".parse().unwrap(),
            Some(&origin),
            false
        ));
    }

    #[test]
    fn non_loopback_listener_requires_auth_by_default() {
        assert!(default_requires_auth(
            "0.0.0.0".parse().unwrap(),
            None,
            false
        ));
    }

    #[test]
    fn non_loopback_cors_origin_requires_auth_by_default() {
        let origin = HeaderValue::from_static("https://example.com");

        assert!(default_requires_auth(
            "127.0.0.1".parse().unwrap(),
            Some(&origin),
            false
        ));
    }

    #[test]
    fn dangerous_host_policy_requires_auth_by_default() {
        assert!(default_requires_auth(
            "127.0.0.1".parse().unwrap(),
            None,
            true
        ));
    }
}
