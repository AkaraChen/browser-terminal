use axum::{
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::state::AppState;

pub(crate) async fn security_middleware(
    State(state): State<AppState>,
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
