use super::*;
use crate::core::production_map::ProductionMapDefinition;

pub async fn production_maps(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AdminError> {
    let principal = authorize_any_capability(
        &state,
        &headers,
        &[Capability::AdminAccess, Capability::CatalogItemGroupManage],
    )
    .await?;
    if !matches!(method, Method::GET | Method::PUT) {
        return Err(method_not_allowed());
    }
    match method {
        Method::GET => {
            require_capability(&state, &principal, Capability::AdminAccess).await?;
            state
                .production_maps
                .maps()
                .await
                .map(json_response)
                .map_err(|_| server_error("production maps fetch failed"))
        }
        Method::PUT => {
            require_capability(&state, &principal, Capability::AdminAccess).await?;
            let input: ProductionMapDefinition = parse_json(&body)?;
            match state.production_maps.upsert_map(input).await {
                Ok(saved) => Ok(json_response(saved)),
                Err(error) => Err(bad_request(error.to_string())),
            }
        }
        _ => Err(method_not_allowed()),
    }
}
