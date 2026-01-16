use std::sync::Arc;

use crate::config::AppConfig;
use crate::core::auth::service::AuthService;
use crate::core::session::manager::SessionManager;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub auth: AuthService,
    pub sessions: SessionManager,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        let auth = AuthService::new(&config);
        let sessions = SessionManager::persistent(
            config.session_store_path.clone(),
            config.session_ttl_seconds,
        );

        Self {
            config: Arc::new(config),
            auth,
            sessions,
        }
    }
}
