use std::sync::mpsc as std_mpsc;

use anyhow::{Context, Result};
use axum::{
    extract::{
        Path, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use portable_pty::PtySize;
use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::pty::{PtyProcess, spawn_pty_reader, spawn_pty_writer};

pub(crate) async fn ws_handler(ws: WebSocketUpgrade, Path(channel): Path<String>) -> Response {
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
    if let Err(err) = pty_control.kill_child() {
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
