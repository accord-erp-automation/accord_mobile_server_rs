use std::sync::Mutex;

use async_trait::async_trait;

use super::ports::{PurchaseReceiptDraft, SupplierPurchaseReceiptLookup, WerkaPortError};
use super::service::WerkaService;

#[tokio::test]
async fn supplier_summary_counts_erp_receipt_statuses_like_go_fallback() {
    let service = WerkaService::new().with_supplier_purchase_receipt_lookup(std::sync::Arc::new(
        FakeSupplierReceipts {
            calls: Mutex::new(Vec::new()),
            receipts: vec![
                receipt(
                    "PR-PENDING",
                    0,
                    "To Bill",
                    5.0,
                    "TG:+998:20260126090000:5.0000",
                ),
                receipt("PR-DRAFT", 0, "Draft", 5.0, "TG:+998:20260126090001:5.0000"),
                receipt(
                    "PR-OK",
                    1,
                    "Completed",
                    5.0,
                    "TG:+998:20260126090002:5.0000",
                ),
                receipt(
                    "PR-PARTIAL",
                    1,
                    "Completed",
                    3.0,
                    "TG:+998:20260126090003:5.0000",
                ),
                receipt(
                    "PR-CANCELLED",
                    2,
                    "Cancelled",
                    5.0,
                    "TG:+998:20260126090004:5.0000",
                ),
            ],
        },
    ));

    let summary = service
        .supplier_summary("SUP-001", "Supplier")
        .await
        .expect("summary result")
        .expect("summary");

    assert_eq!(summary.pending_count, 2);
    assert_eq!(summary.submitted_count, 1);
    assert_eq!(summary.returned_count, 2);
}

struct FakeSupplierReceipts {
    calls: Mutex<Vec<(usize, usize)>>,
    receipts: Vec<PurchaseReceiptDraft>,
}

#[async_trait]
impl SupplierPurchaseReceiptLookup for FakeSupplierReceipts {
    async fn list_supplier_purchase_receipts_page(
        &self,
        _supplier_ref: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<PurchaseReceiptDraft>, WerkaPortError> {
        self.calls.lock().expect("calls").push((limit, offset));
        Ok(self.receipts.clone())
    }
}

fn receipt(
    name: &str,
    doc_status: i32,
    status: &str,
    qty: f64,
    marker: &str,
) -> PurchaseReceiptDraft {
    PurchaseReceiptDraft {
        name: name.to_string(),
        doc_status,
        status: status.to_string(),
        supplier: "SUP-001".to_string(),
        supplier_name: "Supplier".to_string(),
        posting_date: "2026-01-26".to_string(),
        supplier_delivery_note: marker.to_string(),
        item_code: "ITEM-001".to_string(),
        item_name: "Item".to_string(),
        qty,
        uom: "Nos".to_string(),
        ..PurchaseReceiptDraft::default()
    }
}
