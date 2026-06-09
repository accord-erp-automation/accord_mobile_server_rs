use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use rusqlite::{Connection, params};

use crate::core::production_map::{
    ProductionMapDefinition, ProductionMapError, ProductionMapStorePort,
};

#[derive(Clone)]
pub struct ProductionMapStore {
    conn: Arc<Mutex<Connection>>,
}

impl ProductionMapStore {
    pub fn new(path: PathBuf) -> Self {
        Self::open(path).unwrap_or_else(|error| {
            panic!("production map sqlite store unavailable: {error}");
        })
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, ProductionMapError> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(|_| ProductionMapError::StoreFailed)?;
        }
        let conn = Connection::open(path).map_err(|_| ProductionMapError::StoreFailed)?;
        configure_connection(&conn).map_err(|_| ProductionMapError::StoreFailed)?;
        migrate(&conn).map_err(|_| ProductionMapError::StoreFailed)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }
}

#[async_trait]
impl ProductionMapStorePort for ProductionMapStore {
    async fn maps(&self) -> Result<Vec<ProductionMapDefinition>, ProductionMapError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| ProductionMapError::StoreFailed)?;
        let mut stmt = conn
            .prepare(
                "SELECT payload_json
                 FROM production_maps
                 ORDER BY saved_at DESC",
            )
            .map_err(|_| ProductionMapError::StoreFailed)?;
        let rows = stmt
            .query_map([], |row| {
                let payload: String = row.get(0)?;
                let map = serde_json::from_str::<ProductionMapDefinition>(&payload)
                    .map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))?;
                Ok(map)
            })
            .map_err(|_| ProductionMapError::StoreFailed)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| ProductionMapError::StoreFailed)
    }

    async fn put_map(&self, map: ProductionMapDefinition) -> Result<(), ProductionMapError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| ProductionMapError::StoreFailed)?;
        let payload = serde_json::to_string(&map).map_err(|_| ProductionMapError::StoreFailed)?;
        conn.execute(
            "INSERT INTO production_maps
                (id, product_code, title, saved_at, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                product_code = excluded.product_code,
                title = excluded.title,
                saved_at = excluded.saved_at,
                payload_json = excluded.payload_json",
            params![
                map.id.trim(),
                map.product_code.trim(),
                map.title.trim(),
                unix_micros().to_string(),
                payload
            ],
        )
        .map_err(|_| ProductionMapError::StoreFailed)?;
        Ok(())
    }
}

fn configure_connection(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS production_maps (
            id TEXT PRIMARY KEY,
            product_code TEXT NOT NULL,
            title TEXT NOT NULL,
            saved_at TEXT NOT NULL,
            payload_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_production_maps_saved
            ON production_maps(saved_at DESC);
        CREATE INDEX IF NOT EXISTS idx_production_maps_product_code
            ON production_maps(product_code);",
    )
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
    use crate::core::production_map::{
        ProductionMapNode, ProductionMapNodeKind, ProductionMapService,
    };

    #[tokio::test]
    async fn production_map_store_persists_maps_in_sqlite() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("mobile_production_maps.sqlite");
        let service = ProductionMapService::new(Arc::new(ProductionMapStore::new(path.clone())));

        service
            .upsert_map(ProductionMapDefinition {
                id: "map-1".to_string(),
                product_code: "HOT".to_string(),
                title: "Hot".to_string(),
                order_number: "1234".to_string(),
                nodes: vec![
                    ProductionMapNode {
                        id: "start".to_string(),
                        kind: ProductionMapNodeKind::Start,
                        title: "Start".to_string(),
                        formula: None,
                        role_code: String::new(),
                        item_code: String::new(),
                        qty_formula: String::new(),
                        from_location: String::new(),
                        to_location: String::new(),
                        x: 0.0,
                        y: 0.0,
                    },
                    ProductionMapNode {
                        id: "apparatus".to_string(),
                        kind: ProductionMapNodeKind::Apparatus,
                        title: "Extrujen aparat - A".to_string(),
                        formula: None,
                        role_code: String::new(),
                        item_code: String::new(),
                        qty_formula: String::new(),
                        from_location: String::new(),
                        to_location: String::new(),
                        x: 0.0,
                        y: 132.0,
                    },
                    ProductionMapNode {
                        id: "end".to_string(),
                        kind: ProductionMapNodeKind::End,
                        title: "End".to_string(),
                        formula: None,
                        role_code: String::new(),
                        item_code: String::new(),
                        qty_formula: String::new(),
                        from_location: String::new(),
                        to_location: String::new(),
                        x: 0.0,
                        y: 264.0,
                    },
                ],
                edges: vec![
                    crate::core::production_map::ProductionMapEdge {
                        from: "start".to_string(),
                        to: "apparatus".to_string(),
                        branch: String::new(),
                    },
                    crate::core::production_map::ProductionMapEdge {
                        from: "apparatus".to_string(),
                        to: "end".to_string(),
                        branch: String::new(),
                    },
                ],
            })
            .await
            .expect("save map");
        drop(service);

        let conn = rusqlite::Connection::open(&path).expect("open sqlite");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM production_maps", [], |row| row.get(0))
            .expect("count maps");
        assert_eq!(count, 1);
        drop(conn);

        let reloaded = ProductionMapService::new(Arc::new(ProductionMapStore::new(path)));
        let maps = reloaded.maps().await.expect("maps");
        assert_eq!(maps.len(), 1);
        assert_eq!(maps[0].map.product_code, "HOT");
        assert_eq!(maps[0].map.order_number, "1234");
        assert_eq!(maps[0].program.operations.len(), 3);
        assert_eq!(maps[0].program.operations[1].op_code, "apparatus");
    }
}
