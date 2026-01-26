use std::collections::HashSet;

use crate::core::werka::models::SupplierHomeSummary;
use crate::core::werka::ports::{PurchaseReceiptDraft, WerkaPortError};
use crate::core::werka::service::WerkaService;
use crate::core::werka::unannounced::purchase_receipt_to_dispatch_record;

const SUPPLIER_RECEIPT_PAGE_SIZE: usize = 200;

impl WerkaService {
    pub async fn supplier_summary(
        &self,
        supplier_ref: &str,
        supplier_display_name: &str,
    ) -> Result<Option<SupplierHomeSummary>, WerkaPortError> {
        if let Some(lookup) = &self.supplier_read_lookup {
            return lookup.supplier_summary(supplier_ref).await.map(Some);
        }
        let Some(lookup) = &self.supplier_purchase_receipt_lookup else {
            return Ok(None);
        };

        let receipts = collect_supplier_purchase_receipts(lookup.as_ref(), supplier_ref).await?;
        Ok(Some(build_supplier_summary_from_receipts(
            receipts,
            supplier_display_name,
        )))
    }
}

async fn collect_supplier_purchase_receipts(
    lookup: &dyn crate::core::werka::ports::SupplierPurchaseReceiptLookup,
    supplier_ref: &str,
) -> Result<Vec<PurchaseReceiptDraft>, WerkaPortError> {
    let mut result = Vec::with_capacity(SUPPLIER_RECEIPT_PAGE_SIZE);
    let mut seen = HashSet::with_capacity(SUPPLIER_RECEIPT_PAGE_SIZE);
    let mut offset = 0;
    loop {
        let items = lookup
            .list_supplier_purchase_receipts_page(supplier_ref, SUPPLIER_RECEIPT_PAGE_SIZE, offset)
            .await?;
        for item in &items {
            let name = item.name.trim();
            if !name.is_empty() && seen.insert(name.to_string()) {
                result.push(item.clone());
            }
        }
        if items.len() < SUPPLIER_RECEIPT_PAGE_SIZE {
            return Ok(result);
        }
        offset += SUPPLIER_RECEIPT_PAGE_SIZE;
    }
}

fn build_supplier_summary_from_receipts(
    receipts: Vec<PurchaseReceiptDraft>,
    supplier_display_name: &str,
) -> SupplierHomeSummary {
    let mut summary = SupplierHomeSummary::default();
    for receipt in receipts {
        let record = purchase_receipt_to_dispatch_record(receipt, supplier_display_name);
        match record.status.as_str() {
            "pending" | "draft" => summary.pending_count += 1,
            "accepted" => summary.submitted_count += 1,
            "partial" | "rejected" | "cancelled" => summary.returned_count += 1,
            _ => {}
        }
    }
    summary
}
