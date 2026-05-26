use super::*;
use crate::core::production_map::{ProductionMapDefinition, ProductionMapRunRequest};

pub async fn production_maps(
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
    if !matches!(method, Method::GET | Method::PUT) {
        return Err(method_not_allowed());
    }
    match method {
        Method::GET => state
            .production_maps
            .maps()
            .await
            .map(json_response)
            .map_err(|_| server_error("production maps fetch failed")),
        Method::PUT => {
            let input: ProductionMapDefinition = parse_json(&body)?;
            match state.production_maps.upsert_map(input).await {
                Ok(saved) => Ok(json_response(saved)),
                Err(error) => Err(bad_request(error.to_string())),
            }
        }
        _ => Err(method_not_allowed()),
    }
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
