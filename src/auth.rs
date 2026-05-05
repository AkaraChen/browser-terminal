use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use axum::http::{HeaderMap, header};
use base64::{Engine as _, engine::general_purpose};
use rand::distr::{Alphanumeric, SampleString};
use tracing::debug;

const CONFIG_FILE_NAME: &str = ".browser-terminalrc";
const DEFAULT_USERNAME: &str = "admin";
const GENERATED_PASSWORD_LEN: usize = 24;

#[derive(Clone, Debug)]
pub(crate) struct BasicAuth {
    credentials: Option<BasicAuthCredentials>,
}

#[derive(Clone, Debug)]
pub(crate) struct BasicAuthCredentials {
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) password_source: PasswordSource,
}

#[derive(Clone, Debug)]
pub(crate) enum PasswordSource {
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
    pub(crate) fn load(default_requires_auth: bool) -> Result<Self> {
        let config = ConfigFile::load()?;
        Ok(Self::from_config(config, default_requires_auth))
    }

    pub(crate) fn credentials(&self) -> Option<&BasicAuthCredentials> {
        self.credentials.as_ref()
    }

    pub(crate) fn allows_headers(&self, headers: &HeaderMap) -> bool {
        let Some(credentials) = &self.credentials else {
            return true;
        };

        credentials.allows_headers(headers)
    }

    fn from_config(config: Option<ConfigFile>, default_requires_auth: bool) -> Self {
        let username = config
            .as_ref()
            .and_then(|config| config.username.clone())
            .unwrap_or_else(|| DEFAULT_USERNAME.to_string());

        let has_configured_password = config
            .as_ref()
            .and_then(|config| config.password.as_ref())
            .is_some();
        if !default_requires_auth && !has_configured_password {
            return Self { credentials: None };
        }

        let (password, password_source) = if let Some(config) = config {
            if let Some(password) = config.password {
                (password, PasswordSource::Config(config.path))
            } else {
                (generate_password(), PasswordSource::Generated)
            }
        } else {
            (generate_password(), PasswordSource::Generated)
        };

        Self {
            credentials: Some(BasicAuthCredentials {
                username,
                password,
                password_source,
            }),
        }
    }
}

impl BasicAuthCredentials {
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

#[cfg(test)]
impl BasicAuth {
    fn enabled_for_test(username: &str, password: &str) -> Self {
        Self {
            credentials: Some(BasicAuthCredentials {
                username: username.to_string(),
                password: password.to_string(),
                password_source: PasswordSource::Generated,
            }),
        }
    }

    fn disabled_for_test() -> Self {
        Self { credentials: None }
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderValue, header};

    #[test]
    fn basic_auth_accepts_expected_credentials() {
        let auth = BasicAuth::enabled_for_test("admin", "secret");
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, basic_auth_header("admin", "secret"));

        assert!(auth.allows_headers(&headers));
    }

    #[test]
    fn basic_auth_rejects_missing_credentials() {
        let auth = BasicAuth::enabled_for_test("admin", "secret");

        assert!(!auth.allows_headers(&HeaderMap::new()));
    }

    #[test]
    fn disabled_basic_auth_accepts_missing_credentials() {
        let auth = BasicAuth::disabled_for_test();

        assert!(auth.allows_headers(&HeaderMap::new()));
    }

    #[test]
    fn loopback_default_disables_auth_without_configured_password() {
        let auth = BasicAuth::from_config(None, false);

        assert!(auth.credentials().is_none());
    }

    #[test]
    fn configured_password_enables_auth_even_when_loopback_default_is_disabled() {
        let auth = BasicAuth::from_config(
            Some(ConfigFile {
                path: PathBuf::from("/tmp/test-browser-terminalrc"),
                username: Some("admin".to_string()),
                password: Some("fixed-password".to_string()),
            }),
            false,
        );

        assert!(auth.credentials().is_some());
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
