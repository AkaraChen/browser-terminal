use std::net::{IpAddr, SocketAddr};

use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub(crate) struct BrowserLauncher {
    base_url: String,
}

impl BrowserLauncher {
    pub(crate) fn new(local_addr: SocketAddr) -> Self {
        Self {
            base_url: format!(
                "http://{}:{}",
                browser_host(local_addr.ip()),
                local_addr.port()
            ),
        }
    }

    pub(crate) fn root_url(&self) -> String {
        format!("{}/", self.base_url)
    }

    pub(crate) fn open_root(&self) -> Result<()> {
        self.open_url(&self.root_url())
    }

    pub(crate) fn open_channel(&self, channel: &str) -> Result<String> {
        let url = format!("{}/?channel={channel}", self.base_url);
        self.open_url(&url)?;
        Ok(url)
    }

    fn open_url(&self, url: &str) -> Result<()> {
        open::that(url).with_context(|| format!("failed to open {url}"))?;
        Ok(())
    }
}

fn browser_host(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(address) if address.is_unspecified() => "127.0.0.1".to_string(),
        IpAddr::V6(address) if address.is_unspecified() => "[::1]".to_string(),
        IpAddr::V4(address) => address.to_string(),
        IpAddr::V6(address) => format!("[{address}]"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_url_uses_loopback_for_unspecified_ipv4_listener() {
        let launcher = BrowserLauncher::new("0.0.0.0:3000".parse().unwrap());

        assert_eq!(launcher.root_url(), "http://127.0.0.1:3000/");
    }

    #[test]
    fn browser_url_brackets_ipv6_listener() {
        let launcher = BrowserLauncher::new("[::1]:3000".parse().unwrap());

        assert_eq!(launcher.root_url(), "http://[::1]:3000/");
    }
}
