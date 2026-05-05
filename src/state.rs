use crate::{
    auth::BasicAuth, browser::BrowserLauncher, security::SecurityPolicy, session::SessionStore,
};

#[derive(Clone, Debug)]
pub(crate) struct AppState {
    pub(crate) auth: BasicAuth,
    pub(crate) browser: BrowserLauncher,
    pub(crate) security: SecurityPolicy,
    pub(crate) sessions: SessionStore,
}
