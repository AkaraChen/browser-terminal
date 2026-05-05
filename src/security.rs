use std::net::IpAddr;

use axum::http::{HeaderMap, HeaderValue, Method, Uri, header, uri::Authority};
use tower_http::cors::{AllowOrigin, CorsLayer};

#[derive(Clone, Debug)]
pub(crate) struct SecurityPolicy {
    origin_policy: OriginPolicy,
    host_policy: HostPolicy,
}

impl SecurityPolicy {
    pub(crate) fn new(
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

    pub(crate) fn allows_headers(&self, headers: &HeaderMap) -> bool {
        self.host_policy.allows_headers(headers) && self.origin_policy.allows_headers(headers)
    }

    pub(crate) fn description(&self) -> String {
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

    fn allow_origin(&self) -> AllowOrigin {
        self.origin_policy.allow_origin()
    }
}

pub(crate) fn cors_layer(security: SecurityPolicy) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(security.allow_origin())
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .allow_credentials(true)
}

pub(crate) fn parse_cors_origin(value: &str) -> std::result::Result<HeaderValue, String> {
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

pub(crate) fn origin_host_is_loopback(origin: &HeaderValue) -> bool {
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

    authority_host_is_loopback(authority)
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
}
