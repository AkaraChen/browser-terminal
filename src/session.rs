use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use anyhow::{Context, Result, bail};

#[derive(Clone, Debug, Default)]
pub(crate) struct SessionStore {
    inner: Arc<RwLock<SessionStoreInner>>,
}

#[derive(Debug, Default)]
struct SessionStoreInner {
    active: HashMap<String, ActiveSession>,
    pending_start_dirs: HashMap<String, PathBuf>,
}

#[derive(Debug)]
struct ActiveSession {
    process_id: u32,
}

#[derive(Debug)]
pub(crate) enum SessionCwdError {
    NotFound,
    LookupFailed(anyhow::Error),
}

impl SessionStore {
    pub(crate) fn register(&self, channel: String, process_id: Option<u32>) {
        let Some(process_id) = process_id else {
            return;
        };

        self.write_inner()
            .active
            .insert(channel, ActiveSession { process_id });
    }

    pub(crate) fn unregister(&self, channel: &str, process_id: Option<u32>) {
        let Some(process_id) = process_id else {
            return;
        };

        let mut inner = self.write_inner();
        if inner
            .active
            .get(channel)
            .is_some_and(|session| session.process_id == process_id)
        {
            inner.active.remove(channel);
        }
    }

    pub(crate) fn cwd_for_channel(
        &self,
        channel: &str,
    ) -> std::result::Result<PathBuf, SessionCwdError> {
        let process_id = self
            .read_inner()
            .active
            .get(channel)
            .map(|session| session.process_id)
            .ok_or(SessionCwdError::NotFound)?;

        process_cwd(process_id).map_err(SessionCwdError::LookupFailed)
    }

    pub(crate) fn set_start_dir(&self, channel: String, start_dir: PathBuf) {
        self.write_inner()
            .pending_start_dirs
            .insert(channel, start_dir);
    }

    pub(crate) fn clear_start_dir(&self, channel: &str) {
        self.write_inner().pending_start_dirs.remove(channel);
    }

    pub(crate) fn take_start_dir(&self, channel: &str) -> Option<PathBuf> {
        self.write_inner().pending_start_dirs.remove(channel)
    }

    fn read_inner(&self) -> RwLockReadGuard<'_, SessionStoreInner> {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_inner(&self) -> RwLockWriteGuard<'_, SessionStoreInner> {
        self.inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(target_os = "linux")]
fn process_cwd(process_id: u32) -> Result<PathBuf> {
    std::fs::read_link(format!("/proc/{process_id}/cwd"))
        .with_context(|| format!("failed to read cwd for process {process_id}"))
}

#[cfg(target_os = "macos")]
fn process_cwd(process_id: u32) -> Result<PathBuf> {
    let output = std::process::Command::new("lsof")
        .args(["-a", "-d", "cwd", "-Fn", "-p", &process_id.to_string()])
        .output()
        .context("failed to run lsof")?;

    if !output.status.success() {
        bail!(
            "lsof failed for process {process_id}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix('n') {
            return Ok(PathBuf::from(path));
        }
    }

    bail!("lsof did not report cwd for process {process_id}");
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn process_cwd(process_id: u32) -> Result<PathBuf> {
    bail!("process cwd lookup is unsupported for process {process_id} on this platform");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_start_dir_is_consumed_once() {
        let sessions = SessionStore::default();
        let channel = "next".to_string();
        let start_dir = PathBuf::from("/tmp");

        sessions.set_start_dir(channel.clone(), start_dir.clone());

        assert_eq!(sessions.take_start_dir(&channel), Some(start_dir));
        assert_eq!(sessions.take_start_dir(&channel), None);
    }

    #[test]
    fn reads_current_process_cwd() {
        let expected = std::env::current_dir().unwrap().canonicalize().unwrap();
        let actual = process_cwd(std::process::id())
            .unwrap()
            .canonicalize()
            .unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn active_session_unregister_requires_matching_process_id() {
        let sessions = SessionStore::default();
        let channel = "main".to_string();
        let process_id = std::process::id();

        sessions.register(channel.clone(), Some(process_id));
        sessions.unregister(&channel, Some(process_id + 1));
        assert!(sessions.cwd_for_channel(&channel).is_ok());

        sessions.unregister(&channel, Some(process_id));
        assert!(matches!(
            sessions.cwd_for_channel(&channel),
            Err(SessionCwdError::NotFound)
        ));
    }
}
