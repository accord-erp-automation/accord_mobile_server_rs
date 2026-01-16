use async_trait::async_trait;

use crate::core::werka::models::{WerkaHomeData, WerkaHomeSummary};

#[async_trait]
pub trait WerkaHomeLookup: Send + Sync {
    async fn werka_summary(&self) -> Result<WerkaHomeSummary, WerkaPortError>;
    async fn werka_home(&self, pending_limit: usize) -> Result<WerkaHomeData, WerkaPortError>;
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum WerkaPortError {
    #[error("lookup failed")]
    LookupFailed,
    #[error("database lookup failed: {0}")]
    Database(String),
}
