use axum::http::HeaderValue;
use clap::Parser;

use crate::security::parse_cors_origin;

#[derive(Debug, Parser)]
#[command(author, version, about)]
pub(crate) struct Args {
    /// Host name or address to bind the server to.
    #[arg(long, env = "HOST", default_value = "127.0.0.1")]
    pub(crate) host: String,

    /// Port to bind the server to.
    #[arg(long, env = "PORT", default_value_t = 3000)]
    pub(crate) port: u16,

    /// CORS origin to allow, for example http://localhost:5173.
    ///
    /// When omitted, only loopback origins on this server's port are allowed.
    #[arg(long, env = "CORS_ORIGIN", value_parser = parse_cors_origin)]
    pub(crate) cors_origin: Option<HeaderValue>,

    /// Allow arbitrary Host and Origin headers.
    ///
    /// This broadens DNS rebinding exposure and should only be used when you
    /// understand that Basic Auth is the remaining protection.
    #[arg(long, env = "DANGEROUS_ALLOW_ALL_HOST", default_value_t = false)]
    pub(crate) dangerous_allow_all_host: bool,
}
