use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::core::production_map::{
    ProductionMapDefinition, ProductionMapError, ProductionMapStorePort,
};
use crate::store::json_file::{read_map, write_pretty};

#[derive(Clone)]
pub struct ProductionMapStore {
    path: PathBuf,
    state: Arc<Mutex<ProductionMapStoreState>>,
}

#[derive(Default)]
struct ProductionMapStoreState {
    loaded: bool,
    maps: BTreeMap<String, ProductionMapDefinition>,
}

impl ProductionMapStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            state: Arc::new(Mutex::new(ProductionMapStoreState::default())),
        }
    }
}

#[async_trait]
impl ProductionMapStorePort for ProductionMapStore {
    async fn maps(&self) -> Result<Vec<ProductionMapDefinition>, ProductionMapError> {
        let mut state = self.state.lock().await;
        load_if_needed(&self.path, &mut state).await?;
        Ok(state.maps.values().cloned().collect())
    }

    async fn put_map(&self, map: ProductionMapDefinition) -> Result<(), ProductionMapError> {
        let mut state = self.state.lock().await;
        load_if_needed(&self.path, &mut state).await?;
        state.maps.insert(map.id.clone(), map);
        save(&self.path, &state).await
    }
}

async fn load_if_needed(
    path: &Path,
    state: &mut ProductionMapStoreState,
) -> Result<(), ProductionMapError> {
    if state.loaded {
        return Ok(());
    }
    state.maps = read_map(path)
        .await
        .map_err(|_| ProductionMapError::StoreFailed)?;
    state.loaded = true;
    Ok(())
}

async fn save(path: &Path, state: &ProductionMapStoreState) -> Result<(), ProductionMapError> {
    write_pretty(path, &state.maps)
        .await
        .map_err(|_| ProductionMapError::StoreFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::production_map::{
        ProductionMapNode, ProductionMapNodeKind, ProductionMapService,
    };

    #[tokio::test]
    async fn production_map_store_persists_maps() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("maps.json");
        let service = ProductionMapService::new(Arc::new(ProductionMapStore::new(path.clone())));

        service
            .upsert_map(ProductionMapDefinition {
                id: "map-1".to_string(),
                product_code: "HOT".to_string(),
                title: "Hot".to_string(),
                nodes: vec![
                    ProductionMapNode {
                        id: "start".to_string(),
                        kind: ProductionMapNodeKind::Start,
                        title: "Start".to_string(),
                        formula: None,
                        role_code: String::new(),
                        item_code: String::new(),
                        x: 0.0,
                        y: 0.0,
                    },
                    ProductionMapNode {
                        id: "end".to_string(),
                        kind: ProductionMapNodeKind::End,
                        title: "End".to_string(),
                        formula: None,
                        role_code: String::new(),
                        item_code: String::new(),
                        x: 0.0,
                        y: 0.0,
                    },
                ],
                edges: vec![crate::core::production_map::ProductionMapEdge {
                    from: "start".to_string(),
                    to: "end".to_string(),
                    branch: String::new(),
                }],
            })
            .await
            .expect("save map");
        drop(service);

        let reloaded = ProductionMapService::new(Arc::new(ProductionMapStore::new(path)));
        let maps = reloaded.maps().await.expect("maps");
        assert_eq!(maps.len(), 1);
        assert_eq!(maps[0].map.product_code, "HOT");
        assert_eq!(maps[0].program.operations.len(), 2);
    }
}
