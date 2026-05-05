mod auth;
mod cli;
mod guard;
mod pty;
mod security;
mod static_assets;
mod ws;

use anyhow::{Context, Result};
use axum::{Router, middleware, routing::get};
use clap::Parser;
use tracing::info;

use crate::{
    auth::BasicAuth,
    cli::Args,
    guard::{AppState, security_middleware},
    security::{SecurityPolicy, cors_layer},
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
    println!("basic auth username: {}", auth.username);
    println!("basic auth password: {}", auth.password);
    println!("basic auth password source: {}", auth.password_source);
    println!("allowed host/origin policy: {}", security.description());
    info!(%local_addr, "server started");

    axum::serve(listener, app).await.context("server failed")
}
