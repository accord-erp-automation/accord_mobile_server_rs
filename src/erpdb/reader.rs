use async_trait::async_trait;
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions};
use sqlx::{MySqlPool, query_as};

use crate::config::DirectDbConfig;
use crate::core::werka::models::{WerkaHomeData, WerkaHomeSummary};
use crate::core::werka::ports::{WerkaHomeLookup, WerkaPortError};
use crate::erpdb::werka_home::{
    DeliveryNoteSummaryRow, PurchaseReceiptSummaryRow, build_werka_home,
};
use crate::erpdb::werka_summary::{
    DeliveryNoteStatusRow, PurchaseReceiptStatusRow, build_werka_summary,
};

#[derive(Clone)]
pub struct DirectDbReader {
    pool: MySqlPool,
}

impl DirectDbReader {
    pub fn new(config: DirectDbConfig) -> Self {
        let options = MySqlConnectOptions::new()
            .host(&config.host)
            .port(config.port)
            .username(&config.user)
            .password(&config.password)
            .database(&config.name);
        let pool = MySqlPoolOptions::new()
            .max_connections(12)
            .connect_lazy_with(options);

        Self { pool }
    }

    async fn home(&self, pending_limit: usize) -> Result<WerkaHomeData, sqlx::Error> {
        let receipts = query_as::<_, PurchaseReceiptSummaryRow>(PURCHASE_RECEIPT_ROWS_SQL)
            .fetch_all(&self.pool)
            .await?;
        let delivery_notes = query_as::<_, DeliveryNoteSummaryRow>(DELIVERY_NOTE_ROWS_SQL)
            .fetch_all(&self.pool)
            .await?;

        Ok(build_werka_home(&receipts, &delivery_notes, pending_limit))
    }

    async fn summary(&self) -> Result<WerkaHomeSummary, sqlx::Error> {
        let receipts = query_as::<_, PurchaseReceiptStatusRow>(PURCHASE_RECEIPT_STATUS_ROWS_SQL)
            .fetch_all(&self.pool)
            .await?;
        let delivery_notes = query_as::<_, DeliveryNoteStatusRow>(DELIVERY_NOTE_STATUS_ROWS_SQL)
            .fetch_all(&self.pool)
            .await?;

        Ok(build_werka_summary(&receipts, &delivery_notes))
    }
}

#[async_trait]
impl WerkaHomeLookup for DirectDbReader {
    async fn werka_summary(&self) -> Result<WerkaHomeSummary, WerkaPortError> {
        self.summary()
            .await
            .map_err(|error| WerkaPortError::Database(error.to_string()))
    }

    async fn werka_home(&self, pending_limit: usize) -> Result<WerkaHomeData, WerkaPortError> {
        self.home(pending_limit)
            .await
            .map_err(|error| WerkaPortError::Database(error.to_string()))
    }
}

const PURCHASE_RECEIPT_ROWS_SQL: &str = r#"
    SELECT
        pr.name AS name,
        pr.supplier AS supplier,
        COALESCE(pr.supplier_name, '') AS supplier_name,
        pr.docstatus AS doc_status,
        COALESCE(pr.status, '') AS status,
        COALESCE(pr.total_qty, 0) AS total_qty,
        COALESCE(CAST(pr.posting_date AS CHAR), '') AS posting_date,
        COALESCE(pr.supplier_delivery_note, '') AS supplier_delivery_note,
        COALESCE(pr.remarks, '') AS remarks,
        COALESCE(pr.currency, '') AS currency,
        COALESCE(pri.item_code, '') AS item_code,
        COALESCE(pri.item_name, '') AS item_name,
        COALESCE(pri.uom, '') AS uom,
        COALESCE(pri.amount, 0) AS amount
    FROM `tabPurchase Receipt` pr
    LEFT JOIN `tabPurchase Receipt Item` pri ON pri.parent = pr.name AND pri.idx = 1
    WHERE pr.supplier_delivery_note LIKE 'TG:%'
    ORDER BY pr.name DESC
"#;

const DELIVERY_NOTE_ROWS_SQL: &str = r#"
    SELECT
        dn.name AS name,
        dn.customer AS customer,
        COALESCE(dn.customer_name, '') AS customer_name,
        dn.docstatus AS doc_status,
        COALESCE(CAST(dn.modified AS CHAR), '') AS modified,
        COALESCE(dn.total_qty, 0) AS qty,
        COALESCE(dni.returned_qty, 0) AS returned_qty,
        COALESCE(dn.accord_customer_reason, '') AS customer_reason,
        COALESCE(dni.item_code, '') AS item_code,
        COALESCE(dni.item_name, '') AS item_name,
        COALESCE(dni.uom, '') AS uom,
        COALESCE(dn.accord_flow_state, 0) AS accord_flow_state,
        COALESCE(dn.accord_customer_state, 0) AS accord_customer_state
    FROM `tabDelivery Note` dn
    LEFT JOIN `tabDelivery Note Item` dni ON dni.parent = dn.name AND dni.idx = 1
    ORDER BY dn.name DESC
"#;

const PURCHASE_RECEIPT_STATUS_ROWS_SQL: &str = r#"
    SELECT
        pr.docstatus AS doc_status,
        COALESCE(pr.status, '') AS status,
        COALESCE(pr.total_qty, 0) AS total_qty,
        COALESCE(pr.supplier_delivery_note, '') AS supplier_delivery_note,
        COALESCE(pr.remarks, '') AS remarks
    FROM `tabPurchase Receipt` pr
    WHERE pr.supplier_delivery_note LIKE 'TG:%'
"#;

const DELIVERY_NOTE_STATUS_ROWS_SQL: &str = r#"
    SELECT
        dn.docstatus AS doc_status,
        COALESCE(dn.accord_flow_state, 0) AS accord_flow_state,
        COALESCE(dn.accord_customer_state, 0) AS accord_customer_state
    FROM `tabDelivery Note` dn
"#;
