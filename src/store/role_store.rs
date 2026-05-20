use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::core::authz::{
    RoleAssignment, RoleDefinition, RoleDefinitionStorePort, RoleStoreError, role_assignment_key,
};
use crate::store::json_file::{read_map, write_pretty};

#[derive(Clone)]
pub struct RoleDefinitionStore {
    path: PathBuf,
    state: Arc<Mutex<RoleDefinitionStoreState>>,
}

#[derive(Default)]
struct RoleDefinitionStoreState {
    loaded: bool,
    roles: BTreeMap<String, RoleDefinition>,
    assignments: BTreeMap<String, RoleAssignment>,
}

#[derive(Default, Serialize, Deserialize)]
struct RoleDefinitionStoreFile {
    #[serde(default)]
    roles: BTreeMap<String, RoleDefinition>,
    #[serde(default)]
    assignments: BTreeMap<String, RoleAssignment>,
}

impl RoleDefinitionStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            state: Arc::new(Mutex::new(RoleDefinitionStoreState::default())),
        }
    }
}

#[async_trait]
impl RoleDefinitionStorePort for RoleDefinitionStore {
    async fn role_definitions(&self) -> Result<Vec<RoleDefinition>, RoleStoreError> {
        let mut state = self.state.lock().await;
        load_if_needed(&self.path, &mut state).await?;
        Ok(state.roles.values().cloned().collect())
    }

    async fn put_role_definition(&self, role: RoleDefinition) -> Result<(), RoleStoreError> {
        let mut state = self.state.lock().await;
        load_if_needed(&self.path, &mut state).await?;
        state.roles.insert(role.id.clone(), role);
        save(&self.path, &state).await
    }

    async fn role_assignments(&self) -> Result<Vec<RoleAssignment>, RoleStoreError> {
        let mut state = self.state.lock().await;
        load_if_needed(&self.path, &mut state).await?;
        Ok(state.assignments.values().cloned().collect())
    }

    async fn put_role_assignment(&self, assignment: RoleAssignment) -> Result<(), RoleStoreError> {
        let mut state = self.state.lock().await;
        load_if_needed(&self.path, &mut state).await?;
        state.assignments.insert(
            role_assignment_key(&assignment.principal_role, &assignment.principal_ref),
            assignment,
        );
        save(&self.path, &state).await
    }
}

async fn load_if_needed(
    path: &Path,
    state: &mut RoleDefinitionStoreState,
) -> Result<(), RoleStoreError> {
    if state.loaded {
        return Ok(());
    }
    if tokio::fs::metadata(path).await.is_err() {
        state.loaded = true;
        return Ok(());
    }
    let raw = tokio::fs::read(path)
        .await
        .map_err(|_| RoleStoreError::StoreFailed)?;
    if raw.is_empty() {
        state.loaded = true;
        return Ok(());
    }
    match serde_json::from_slice::<RoleDefinitionStoreFile>(&raw) {
        Ok(file) => {
            state.roles = file.roles;
            state.assignments = file.assignments;
        }
        Err(_) => {
            state.roles = read_map::<RoleDefinition>(path)
                .await
                .map_err(|_| RoleStoreError::StoreFailed)?
                .into_iter()
                .collect();
        }
    }
    state.loaded = true;
    Ok(())
}

async fn save(path: &Path, state: &RoleDefinitionStoreState) -> Result<(), RoleStoreError> {
    write_pretty(
        path,
        &RoleDefinitionStoreFile {
            roles: state.roles.clone(),
            assignments: state.assignments.clone(),
        },
    )
    .await
    .map_err(|_| RoleStoreError::StoreFailed)
}

#[cfg(test)]
mod tests {
    use crate::core::auth::models::PrincipalRole;
    use crate::core::authz::{RoleAssignment, RoleDefinition, RoleDefinitionStorePort};

    use super::RoleDefinitionStore;

    #[tokio::test]
    async fn role_definition_store_persists_custom_roles() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("roles.json");
        let store = RoleDefinitionStore::new(path.clone());

        store
            .put_role_definition(RoleDefinition {
                id: "scale_operator".to_string(),
                label: "Scale operator".to_string(),
                base_role: PrincipalRole::Werka,
                capability_codes: vec!["gscale.print".to_string()],
                system: false,
            })
            .await
            .expect("put role");
        drop(store);

        let reloaded = RoleDefinitionStore::new(path);
        let roles = reloaded.role_definitions().await.expect("role definitions");
        assert_eq!(roles.len(), 1);
        assert_eq!(roles[0].id, "scale_operator");
        assert_eq!(roles[0].capability_codes, vec!["gscale.print"]);
    }

    #[tokio::test]
    async fn role_definition_store_persists_assignments_with_roles() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("roles.json");
        let store = RoleDefinitionStore::new(path.clone());

        store
            .put_role_definition(RoleDefinition {
                id: "catalog_only".to_string(),
                label: "Catalog only".to_string(),
                base_role: PrincipalRole::Werka,
                capability_codes: vec!["gscale.catalog.read".to_string()],
                system: false,
            })
            .await
            .expect("put role");
        store
            .put_role_assignment(RoleAssignment {
                principal_role: PrincipalRole::Werka,
                principal_ref: "werka".to_string(),
                role_id: "catalog_only".to_string(),
            })
            .await
            .expect("put assignment");
        drop(store);

        let reloaded = RoleDefinitionStore::new(path);
        assert_eq!(
            reloaded
                .role_definitions()
                .await
                .expect("roles")
                .first()
                .expect("role")
                .id,
            "catalog_only"
        );
        assert_eq!(
            reloaded
                .role_assignments()
                .await
                .expect("assignments")
                .first()
                .expect("assignment")
                .role_id,
            "catalog_only"
        );
    }
}
