use axum::{
    body::Body,
    http::{HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use include_dir::{Dir, include_dir};

static STATIC_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/static");

pub(crate) async fn static_handler(uri: Uri) -> Response {
    let Some(path) = static_asset_path(&uri) else {
        return static_not_found();
    };
    let Some(file) = STATIC_DIR.get_file(path) else {
        return static_not_found();
    };

    let content_type = mime_guess::from_path(path).first_or_octet_stream();
    let mut response = Response::new(Body::from(file.contents()));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type.as_ref())
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    response
}

fn static_asset_path(uri: &Uri) -> Option<&str> {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if path.contains('\\')
        || path
            .split('/')
            .any(|segment| segment == "." || segment == "..")
    {
        return None;
    }

    Some(path)
}

fn static_not_found() -> Response {
    (StatusCode::NOT_FOUND, "404 Not Found").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_asset_path_maps_root_to_index() {
        let uri = "/".parse::<Uri>().unwrap();

        assert_eq!(static_asset_path(&uri), Some("index.html"));
    }

    #[test]
    fn static_asset_path_rejects_parent_segments() {
        let uri = "/../Cargo.toml".parse::<Uri>().unwrap();

        assert_eq!(static_asset_path(&uri), None);
    }

    #[test]
    fn static_dir_embeds_index_html() {
        assert!(STATIC_DIR.get_file("index.html").is_some());
    }

    #[test]
    fn static_dir_embeds_frontend_modules() {
        assert!(STATIC_DIR.get_file("app.js").is_some());
        assert!(STATIC_DIR.get_file("js/terminal-store.js").is_some());
        assert!(
            STATIC_DIR
                .get_file("js/components/connection-status.js")
                .is_some()
        );
        assert!(STATIC_DIR.get_file("js/config.js").is_some());
        assert!(STATIC_DIR.get_file("js/socket.js").is_some());
    }
}
