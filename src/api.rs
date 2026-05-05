use axum::{Json, extract::State, http::StatusCode};
use rand::distr::{Alphanumeric, SampleString};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{session::SessionCwdError, state::AppState};

#[derive(Debug, Deserialize)]
pub(crate) struct NewWindowRequest {
    channel: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct NewWindowResponse {
    channel: String,
    url: String,
}

pub(crate) async fn new_window_handler(
    State(state): State<AppState>,
    Json(request): Json<NewWindowRequest>,
) -> Result<Json<NewWindowResponse>, (StatusCode, String)> {
    if request.channel.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "channel is required".to_string()));
    }

    let start_dir = match state.sessions.cwd_for_channel(&request.channel) {
        Ok(cwd) => cwd,
        Err(SessionCwdError::NotFound) => {
            return Err((StatusCode::NOT_FOUND, "session not found".to_string()));
        }
        Err(SessionCwdError::LookupFailed(err)) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to inspect session cwd: {err}"),
            ));
        }
    };

    let channel = new_channel_id();
    state
        .sessions
        .set_start_dir(channel.clone(), start_dir.clone());
    let url = match state.browser.open_channel(&channel) {
        Ok(url) => url,
        Err(err) => {
            state.sessions.clear_start_dir(&channel);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to open browser window: {err}"),
            ));
        }
    };

    info!(
        source_channel = %request.channel,
        channel = %channel,
        start_dir = %start_dir.display(),
        url = %url,
        "opened new terminal window"
    );

    Ok(Json(NewWindowResponse { channel, url }))
}

fn new_channel_id() -> String {
    format!(
        "server-{}",
        Alphanumeric.sample_string(&mut rand::rng(), 24)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_channel_ids_are_url_safe() {
        let channel = new_channel_id();

        assert!(channel.starts_with("server-"));
        assert!(
            channel
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
        );
    }
}
