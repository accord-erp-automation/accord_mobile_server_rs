use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension, params};

use crate::core::calculate_orders::{
    CalculateOrderError, CalculateOrderStorePort, CalculateOrderTemplate, validate_template,
};

#[derive(Clone)]
pub struct CalculateOrderStore {
    conn: Arc<Mutex<Connection>>,
}

impl CalculateOrderStore {
    pub fn new(path: PathBuf) -> Self {
        Self::open(path).unwrap_or_else(|error| {
            panic!("calculate order sqlite store unavailable: {error}");
        })
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, CalculateOrderError> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(|_| CalculateOrderError::StoreFailed)?;
        }
        let conn = Connection::open(path).map_err(|_| CalculateOrderError::StoreFailed)?;
        configure_connection(&conn).map_err(|_| CalculateOrderError::StoreFailed)?;
        migrate(&conn).map_err(|_| CalculateOrderError::StoreFailed)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
}

#[async_trait]
impl CalculateOrderStorePort for CalculateOrderStore {
    async fn list(
        &self,
        owner_key: &str,
    ) -> Result<Vec<CalculateOrderTemplate>, CalculateOrderError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| CalculateOrderError::StoreFailed)?;
        let mut stmt = conn
            .prepare(
                "SELECT payload_json
                 FROM calculate_order_templates
                 WHERE owner_key = ?1
                 ORDER BY saved_at DESC",
            )
            .map_err(|_| CalculateOrderError::StoreFailed)?;
        let rows = stmt
            .query_map(params![owner_key.trim()], |row| {
                let payload: String = row.get(0)?;
                let template = serde_json::from_str::<CalculateOrderTemplate>(&payload)
                    .map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))?;
                Ok(template)
            })
            .map_err(|_| CalculateOrderError::StoreFailed)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| CalculateOrderError::StoreFailed)
    }

    async fn upsert(
        &self,
        owner_key: &str,
        template: CalculateOrderTemplate,
    ) -> Result<CalculateOrderTemplate, CalculateOrderError> {
        validate_template(&template)?;
        let conn = self
            .conn
            .lock()
            .map_err(|_| CalculateOrderError::StoreFailed)?;
        let existing = existing_id(&conn, owner_key, &template.name)?;
        let saved = stamp_template(template, existing);
        let lower_name = normalize_name(&saved.name);
        let payload =
            serde_json::to_string(&saved).map_err(|_| CalculateOrderError::StoreFailed)?;
        conn.execute(
            "INSERT INTO calculate_order_templates
                (id, owner_key, name, lower_name, saved_at, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(owner_key, lower_name) DO UPDATE SET
                id = excluded.id,
                name = excluded.name,
                saved_at = excluded.saved_at,
                payload_json = excluded.payload_json",
            params![
                saved.id,
                owner_key.trim(),
                saved.name,
                lower_name,
                saved.saved_at,
                payload
            ],
        )
        .map_err(|_| CalculateOrderError::StoreFailed)?;
        Ok(saved)
    }

    async fn delete(&self, owner_key: &str, id: &str) -> Result<(), CalculateOrderError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| CalculateOrderError::StoreFailed)?;
        conn.execute(
            "DELETE FROM calculate_order_templates WHERE owner_key = ?1 AND id = ?2",
            params![owner_key.trim(), id.trim()],
        )
        .map_err(|_| CalculateOrderError::StoreFailed)?;
        Ok(())
    }
}

fn existing_id(
    conn: &Connection,
    owner_key: &str,
    name: &str,
) -> Result<Option<String>, CalculateOrderError> {
    conn.query_row(
        "SELECT id
         FROM calculate_order_templates
         WHERE owner_key = ?1 AND lower_name = ?2
         LIMIT 1",
        params![owner_key.trim(), normalize_name(name)],
        |row| row.get(0),
    )
    .optional()
    .map_err(|_| CalculateOrderError::StoreFailed)
}

fn stamp_template(
    mut template: CalculateOrderTemplate,
    existing_id: Option<String>,
) -> CalculateOrderTemplate {
    template.id = existing_id
        .filter(|id| !id.trim().is_empty())
        .or_else(|| (!template.id.trim().is_empty()).then(|| template.id.trim().to_string()))
        .unwrap_or_else(new_id);
    template.name = template.name.trim().to_string();
    template.order_number = template.order_number.trim().to_string();
    template.customer = template.customer.trim().to_string();
    template.product = template.product.trim().to_string();
    template.status = template.status.trim().to_string();
    template.material_display = template.material_display.trim().to_string();
    template.color = template.color.trim().to_string();
    template.image_id = template.image_id.trim().to_string();
    template.image_name = template.image_name.trim().to_string();
    template.image_mime = template.image_mime.trim().to_string();
    template.image_url = template.image_url.trim().to_string();
    template.first_layer_material = template.first_layer_material.trim().to_string();
    template.first_layer_micron = template.first_layer_micron.trim().to_string();
    template.second_layer_material = template.second_layer_material.trim().to_string();
    template.second_layer_micron = template.second_layer_micron.trim().to_string();
    template.third_layer_material = template.third_layer_material.trim().to_string();
    template.third_layer_micron = template.third_layer_micron.trim().to_string();
    template.note = template.note.trim().to_string();
    template.saved_at = unix_micros().to_string();
    template
}

fn configure_connection(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS calculate_order_templates (
            id TEXT PRIMARY KEY,
            owner_key TEXT NOT NULL,
            name TEXT NOT NULL,
            lower_name TEXT NOT NULL,
            saved_at TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            UNIQUE(owner_key, lower_name)
        );
        CREATE INDEX IF NOT EXISTS idx_calculate_order_templates_owner_saved
            ON calculate_order_templates(owner_key, saved_at DESC);",
    )
}

fn normalize_name(value: &str) -> String {
    value.trim().to_lowercase()
}

fn new_id() -> String {
    unix_micros().to_string()
}

fn unix_micros() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_micros())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn calculate_order_sqlite_store_round_trips_and_upserts_templates() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("orders.sqlite");
        let store = CalculateOrderStore::new(path.clone());

        let saved = store
            .upsert(
                "admin:admin",
                CalculateOrderTemplate {
                    id: String::new(),
                    name: "CPP 600".to_string(),
                    saved_at: String::new(),
                    order_number: "ORD-1".to_string(),
                    customer: "Mijoz".to_string(),
                    product: "cpp / 20 mikron / 600".to_string(),
                    status: String::new(),
                    material_display: String::new(),
                    color: String::new(),
                    image_id: "img-1".to_string(),
                    image_name: "rang.jpg".to_string(),
                    image_mime: "image/jpeg".to_string(),
                    image_size_bytes: 3,
                    image_url: "/v1/mobile/calculate/orders/image/view?id=img-1".to_string(),
                    width_mm: 530.0,
                    waste_percent: 3.0,
                    roll_count: Some(7.0),
                    first_layer_material: "pet".to_string(),
                    first_layer_micron: "12".to_string(),
                    second_layer_material: "pe oq".to_string(),
                    second_layer_micron: "30".to_string(),
                    third_layer_material: String::new(),
                    third_layer_micron: String::new(),
                    note: String::new(),
                },
            )
            .await
            .expect("save");
        let updated = store
            .upsert(
                "admin:admin",
                CalculateOrderTemplate {
                    width_mm: 630.0,
                    ..saved.clone()
                },
            )
            .await
            .expect("update");

        assert_eq!(updated.id, saved.id);
        assert_eq!(updated.width_mm, 630.0);

        drop(store);
        let conn = rusqlite::Connection::open(&path).expect("open sqlite");
        let row_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM calculate_order_templates",
                [],
                |row| row.get(0),
            )
            .expect("row count");
        assert_eq!(row_count, 1);
        drop(conn);

        let reloaded = CalculateOrderStore::new(path);
        let rows = reloaded.list("admin:admin").await.expect("list");

        assert_eq!(rows, vec![updated.clone()]);
        assert!(
            serde_json::to_value(&rows[0])
                .expect("json")
                .get("kg")
                .is_none()
        );

        reloaded
            .delete("admin:admin", &updated.id)
            .await
            .expect("delete");
        assert!(reloaded.list("admin:admin").await.expect("list").is_empty());
    }
}
