use std::sync::Arc;

use async_trait::async_trait;

use super::models::{DispatchRecord, WerkaHomeData, WerkaHomeSummary};
use super::ports::{WerkaHomeLookup, WerkaPortError};
use super::service::WerkaService;

#[tokio::test]
async fn home_returns_none_without_lookup() {
    let data = WerkaService::new().home(20).await.expect("home result");

    assert!(data.is_none());
}

#[tokio::test]
async fn home_preloads_from_lookup_with_limit() {
    let data = WerkaService::new()
        .with_lookup(Arc::new(FakeWerkaHomeLookup))
        .home(20)
        .await
        .expect("home result")
        .expect("home data");

    assert_eq!(data.summary.pending_count, 1);
    assert_eq!(data.pending_items[0].id, "PR-001");
}

struct FakeWerkaHomeLookup;

#[async_trait]
impl WerkaHomeLookup for FakeWerkaHomeLookup {
    async fn werka_home(&self, pending_limit: usize) -> Result<WerkaHomeData, WerkaPortError> {
        assert_eq!(pending_limit, 20);
        Ok(WerkaHomeData {
            summary: WerkaHomeSummary {
                pending_count: 1,
                confirmed_count: 2,
                returned_count: 3,
            },
            pending_items: vec![DispatchRecord {
                id: "PR-001".to_string(),
                supplier_name: "Supplier".to_string(),
                item_code: "ITEM-001".to_string(),
                item_name: "Item".to_string(),
                uom: "Kg".to_string(),
                sent_qty: 10.0,
                accepted_qty: 0.0,
                status: "pending".to_string(),
                created_label: "2026-01-16T10:00:00Z".to_string(),
                ..DispatchRecord::default()
            }],
        })
    }
}
