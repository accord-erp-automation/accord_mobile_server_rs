use std::sync::Arc;

use crate::config::AppConfig;
use crate::core::auth::service::AuthService;
use crate::core::profile::service::ProfileService;
use crate::core::session::manager::SessionManager;
use crate::core::werka::service::WerkaService;
use crate::erpdb::reader::DirectDbReader;
use crate::erpnext::client::ErpnextClient;
use crate::store::admin_state_store::AdminSupplierStateStore;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub auth: AuthService,
    pub profiles: ProfileService,
    pub werka: WerkaService,
    pub sessions: SessionManager,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        let mut auth = AuthService::new(&config);
        let mut profiles = ProfileService::new(config.erp_url.clone());
        let mut werka = WerkaService::new();
        let sessions = SessionManager::persistent(
            config.session_store_path.clone(),
            config.session_ttl_seconds,
        );

        if config.erp_configured() {
            auth = auth.with_supplier_dependencies(
                Arc::new(ErpnextClient::new(
                    config.erp_url.clone(),
                    config.erp_api_key.clone(),
                    config.erp_api_secret.clone(),
                    config.erp_timeout,
                )),
                Arc::new(AdminSupplierStateStore::new(
                    config.admin_supplier_store_path.clone(),
                )),
            );
            auth = auth.with_customer_dependencies(
                Arc::new(ErpnextClient::new(
                    config.erp_url.clone(),
                    config.erp_api_key.clone(),
                    config.erp_api_secret.clone(),
                    config.erp_timeout,
                )),
                Arc::new(AdminSupplierStateStore::new(
                    config.admin_supplier_store_path.clone(),
                )),
            );
            profiles = profiles.with_erp_lookup(Arc::new(ErpnextClient::new(
                config.erp_url.clone(),
                config.erp_api_key.clone(),
                config.erp_api_secret.clone(),
                config.erp_timeout,
            )));
        }
        match config.direct_db_config() {
            Ok(Some(db_config)) => {
                tracing::info!(
                    host = %db_config.host,
                    port = db_config.port,
                    database = %db_config.name,
                    "direct DB read enabled for Werka home"
                );
                werka = werka.with_lookup(Arc::new(DirectDbReader::new(db_config)));
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(%error, "direct DB read disabled");
            }
        }

        Self {
            config: Arc::new(config),
            auth,
            profiles,
            werka,
            sessions,
        }
    }
}
