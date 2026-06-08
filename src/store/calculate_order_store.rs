use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::core::calculate_orders::{
    CalculateOrderError, CalculateOrderStorePort, CalculateOrderTemplate, upsert_template,
};
use crate::store::json_file::{read_map, write_pretty};

#[derive(Clone)]
pub struct CalculateOrderStore {
    path: PathBuf,
    state: Arc<Mutex<CalculateOrderStoreState>>,
}

#[derive(Default)]
struct CalculateOrderStoreState {
    loaded: bool,
    orders: BTreeMap<String, Vec<CalculateOrderTemplate>>,
}

impl CalculateOrderStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            state: Arc::new(Mutex::new(CalculateOrderStoreState::default())),
        }
    }
}

#[async_trait]
impl CalculateOrderStorePort for CalculateOrderStore {
    async fn list(
        &self,
        owner_key: &str,
    ) -> Result<Vec<CalculateOrderTemplate>, CalculateOrderError> {
        let mut state = self.state.lock().await;
        load_if_needed(&self.path, &mut state).await?;
        Ok(state
            .orders
            .get(owner_key.trim())
            .cloned()
            .unwrap_or_default())
    }

    async fn upsert(
        &self,
        owner_key: &str,
        template: CalculateOrderTemplate,
    ) -> Result<CalculateOrderTemplate, CalculateOrderError> {
        let mut state = self.state.lock().await;
        load_if_needed(&self.path, &mut state).await?;
        let saved = upsert_template(&mut state.orders, owner_key.trim(), template)?;
        save(&self.path, &state).await?;
        Ok(saved)
    }

    async fn delete(&self, owner_key: &str, id: &str) -> Result<(), CalculateOrderError> {
        let mut state = self.state.lock().await;
        load_if_needed(&self.path, &mut state).await?;
        if let Some(list) = state.orders.get_mut(owner_key.trim()) {
            list.retain(|template| template.id != id.trim());
        }
        save(&self.path, &state).await
    }
}

async fn load_if_needed(
    path: &Path,
    state: &mut CalculateOrderStoreState,
) -> Result<(), CalculateOrderError> {
    if state.loaded {
        return Ok(());
    }
    state.orders = read_map(path)
        .await
        .map_err(|_| CalculateOrderError::StoreFailed)?;
    state.loaded = true;
    Ok(())
}

async fn save(path: &Path, state: &CalculateOrderStoreState) -> Result<(), CalculateOrderError> {
    write_pretty(path, &state.orders)
        .await
        .map_err(|_| CalculateOrderError::StoreFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn calculate_order_store_round_trips_templates() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("orders.json");
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

        drop(store);
        let reloaded = CalculateOrderStore::new(path);
        let rows = reloaded.list("admin:admin").await.expect("list");

        assert_eq!(rows, vec![saved]);
        assert!(
            serde_json::to_value(&rows[0])
                .expect("json")
                .get("kg")
                .is_none()
        );
    }
}
