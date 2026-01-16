use async_trait::async_trait;
use reqwest::Method;
use serde::Deserialize;
use serde_json::Value;

use crate::core::werka::ports::{
    PurchaseReceiptComment, PurchaseReceiptDraft, PurchaseReceiptSubmissionResult,
    SupplierUnannouncedWriter, WerkaPortError,
};
use crate::erpnext::client::ErpnextClient;

use super::{ListResponse, ResourceResponse, map_purchase_receipt};

#[async_trait]
impl SupplierUnannouncedWriter for ErpnextClient {
    async fn get_purchase_receipt(
        &self,
        name: &str,
    ) -> Result<PurchaseReceiptDraft, WerkaPortError> {
        ErpnextClient::get_purchase_receipt(self, name).await
    }

    async fn update_purchase_receipt_remarks(
        &self,
        name: &str,
        remarks: &str,
    ) -> Result<(), WerkaPortError> {
        self.request_empty(
            Method::PUT,
            &format!(
                "/api/resource/Purchase Receipt/{}",
                urlencoding::encode(name.trim())
            ),
            Some(serde_json::json!({ "remarks": remarks.trim() })),
        )
        .await
    }

    async fn confirm_and_submit_purchase_receipt(
        &self,
        name: &str,
        accepted_qty: f64,
        returned_qty: f64,
        _return_reason: &str,
        _return_comment: &str,
    ) -> Result<PurchaseReceiptSubmissionResult, WerkaPortError> {
        if accepted_qty < 0.0 {
            return Err(WerkaPortError::WriteFailed(
                "accepted qty cannot be negative".to_string(),
            ));
        }
        let mut doc = self.purchase_receipt_doc(name).await?;
        let draft = map_purchase_receipt(doc.clone())?;
        if accepted_qty > draft.qty {
            return Err(WerkaPortError::WriteFailed(
                "accepted qty cannot exceed sent qty".to_string(),
            ));
        }
        update_first_receipt_item(&mut doc, accepted_qty, returned_qty)?;
        self.request_empty(
            Method::PUT,
            &format!(
                "/api/resource/Purchase Receipt/{}",
                urlencoding::encode(name.trim())
            ),
            Some(doc),
        )
        .await?;
        self.submit_doc("Purchase Receipt", name).await?;
        Ok(PurchaseReceiptSubmissionResult {
            name: name.trim().to_string(),
            supplier: draft.supplier,
            item_code: draft.item_code,
            uom: draft.uom,
            sent_qty: draft.qty,
            accepted_qty,
            supplier_delivery_note: draft.supplier_delivery_note,
            note: String::new(),
        })
    }

    async fn add_purchase_receipt_comment(
        &self,
        name: &str,
        content: &str,
    ) -> Result<(), WerkaPortError> {
        if content.trim().is_empty() {
            return Ok(());
        }
        self.request_empty(
            Method::POST,
            "/api/resource/Comment",
            Some(serde_json::json!({
                "comment_type": "Comment",
                "reference_doctype": "Purchase Receipt",
                "reference_name": name.trim(),
                "content": content.trim(),
            })),
        )
        .await
    }

    async fn list_purchase_receipt_comments(
        &self,
        name: &str,
        limit: usize,
    ) -> Result<Vec<PurchaseReceiptComment>, WerkaPortError> {
        let limit = if limit == 0 || limit > 100 { 50 } else { limit };
        let normalized_name = name.trim().to_string();
        if normalized_name.is_empty() {
            return Ok(Vec::new());
        }
        let filters = serde_json::json!([
            ["reference_doctype", "=", "Purchase Receipt"],
            ["reference_name", "in", [normalized_name]],
            ["comment_type", "=", "Comment"]
        ]);
        let payload: ListResponse<CommentRow> = self
            .purchase_get_json(
                "/api/resource/Comment",
                &[
                    (
                        "fields",
                        r#"["name","content","creation","reference_name"]"#.to_string(),
                    ),
                    ("filters", filters.to_string()),
                    ("order_by", "reference_name asc, creation asc".to_string()),
                    ("limit_page_length", limit.to_string()),
                ],
            )
            .await?;
        Ok(payload
            .data
            .into_iter()
            .filter(|row| row.reference_name.trim() == name.trim())
            .map(|row| PurchaseReceiptComment {
                id: row.name.trim().to_string(),
                content: row.content.trim().to_string(),
                created_at: row.creation.trim().to_string(),
            })
            .collect())
    }
}

impl ErpnextClient {
    async fn purchase_receipt_doc(&self, name: &str) -> Result<Value, WerkaPortError> {
        let payload: ResourceResponse<Value> = self
            .request_json(
                Method::GET,
                &format!(
                    "/api/resource/Purchase Receipt/{}",
                    urlencoding::encode(name.trim())
                ),
                None,
            )
            .await?;
        Ok(payload.data)
    }

    async fn submit_doc(&self, doctype: &str, name: &str) -> Result<(), WerkaPortError> {
        for attempt in 0..2 {
            let latest: ResourceResponse<Value> = self
                .request_json(
                    Method::GET,
                    &format!(
                        "/api/resource/{}/{}",
                        urlencoding::encode(doctype.trim()),
                        urlencoding::encode(name.trim())
                    ),
                    None,
                )
                .await?;
            let result = self
                .request_empty(
                    Method::POST,
                    "/api/method/frappe.client.submit",
                    Some(serde_json::json!({ "doc": latest.data })),
                )
                .await;
            match result {
                Ok(()) => return Ok(()),
                Err(WerkaPortError::WriteFailed(message))
                    if attempt == 0 && message.contains("TimestampMismatchError") =>
                {
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

fn update_first_receipt_item(
    doc: &mut Value,
    accepted_qty: f64,
    returned_qty: f64,
) -> Result<(), WerkaPortError> {
    let first_item = doc
        .get_mut("items")
        .and_then(Value::as_array_mut)
        .and_then(|items| items.first_mut())
        .and_then(Value::as_object_mut)
        .ok_or_else(|| WerkaPortError::WriteFailed("purchase receipt has no items".to_string()))?;
    let conversion_factor = first_item
        .get("conversion_factor")
        .and_then(Value::as_f64)
        .filter(|value| *value > 0.0)
        .unwrap_or(1.0);
    let received_qty = accepted_qty + returned_qty;
    first_item.insert("qty".to_string(), serde_json::json!(accepted_qty));
    first_item.insert("received_qty".to_string(), serde_json::json!(received_qty));
    first_item.insert(
        "stock_qty".to_string(),
        serde_json::json!(accepted_qty * conversion_factor),
    );
    first_item.insert(
        "received_stock_qty".to_string(),
        serde_json::json!(received_qty * conversion_factor),
    );
    first_item.insert("rejected_qty".to_string(), serde_json::json!(returned_qty));
    if returned_qty <= 0.0 {
        first_item.insert("rejected_warehouse".to_string(), serde_json::json!(""));
    }
    first_item.insert(
        "allow_zero_valuation_rate".to_string(),
        serde_json::json!(1),
    );
    if !first_item.contains_key("rate") {
        first_item.insert("rate".to_string(), serde_json::json!(0));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct CommentRow {
    name: String,
    content: String,
    creation: String,
    reference_name: String,
}
