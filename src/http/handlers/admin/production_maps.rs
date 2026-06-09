use super::*;
use crate::core::production_map::{
    ProductionMapDefinition, ProductionMapNode, ProductionMapNodeKind, ProductionMapRunRequest,
    ProductionMapSaved,
};

pub async fn production_maps(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AdminError> {
    let principal = authorize_any_capability(
        &state,
        &headers,
        &[
            Capability::AdminAccess,
            Capability::ProductionMapManage,
            Capability::ApparatusQueueRead,
        ],
    )
    .await?;
    if !matches!(method, Method::GET | Method::PUT) {
        return Err(method_not_allowed());
    }
    match method {
        Method::GET => {
            let maps = state
                .production_maps
                .maps()
                .await
                .map_err(|_| server_error("production maps fetch failed"))?;
            if state
                .admin
                .principal_has_capability(&principal, Capability::ProductionMapManage)
                .await
                || state
                    .admin
                    .principal_has_capability(&principal, Capability::AdminAccess)
                    .await
            {
                return Ok(json_response(maps));
            }
            let allowed = state.admin.principal_assigned_apparatus(&principal).await;
            Ok(json_response(filter_maps_for_apparatus(maps, &allowed)))
        }
        Method::PUT => {
            authorize_any_capability(
                &state,
                &headers,
                &[Capability::AdminAccess, Capability::ProductionMapManage],
            )
            .await?;
            let input: ProductionMapDefinition = parse_json(&body)?;
            match state.production_maps.upsert_map(input).await {
                Ok(saved) => Ok(json_response(saved)),
                Err(error) => Err(bad_request(error.to_string())),
            }
        }
        _ => Err(method_not_allowed()),
    }
}

fn filter_maps_for_apparatus(
    maps: Vec<ProductionMapSaved>,
    assigned_apparatus: &[String],
) -> Vec<ProductionMapSaved> {
    if assigned_apparatus.is_empty() {
        return Vec::new();
    }
    maps.into_iter()
        .filter(|map| {
            map.map
                .nodes
                .iter()
                .any(|node| apparatus_is_allowed(node, assigned_apparatus))
        })
        .collect()
}

fn apparatus_is_allowed(node: &ProductionMapNode, assigned_apparatus: &[String]) -> bool {
    node.kind == ProductionMapNodeKind::Apparatus
        && assigned_apparatus
            .iter()
            .any(|apparatus| apparatus.trim() == node.title.trim())
}

pub async fn production_map_run(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AdminError> {
    authorize_any_capability(
        &state,
        &headers,
        &[Capability::AdminAccess, Capability::ProductionMapManage],
    )
    .await?;
    if method != Method::POST {
        return Err(method_not_allowed());
    }
    let input: ProductionMapRunRequest = parse_json(&body)?;
    match state.production_maps.run_map(input).await {
        Ok(result) => Ok(json_response(result)),
        Err(error) => Err(bad_request(error.to_string())),
    }
}
