use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode};
use serde::Serialize;

use crate::app::AppState;
use crate::core::auth::models::{Principal, PrincipalRole};
use crate::core::authz::Capability;
use crate::core::calculate_orders::{CalculateOrderError, CalculateOrderTemplate, owner_key};
use crate::core::formula::{CalculateRequest, calculate};
use crate::http::handlers::auth::bearer_token;

pub async fn calculate_route(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<CalculateErrorResponse>)> {
    if method != Method::POST {
        return Err(method_not_allowed());
    }
    let principal = authenticated_principal(&state, &headers).await?;
    let can_admin = state
        .admin
        .principal_has_capability(&principal, Capability::AdminAccess)
        .await;
    let can_production_map = state
        .admin
        .principal_has_capability(&principal, Capability::ProductionMapManage)
        .await;
    if !can_admin && !can_production_map {
        return Err(forbidden());
    }
    let request: CalculateRequest =
        serde_json::from_slice(&body).map_err(|_| bad_request("invalid_json", "invalid json"))?;
    let response = calculate(request).map_err(|error| bad_request("invalid_input", error))?;
    Ok(Json(
        serde_json::to_value(response).unwrap_or_else(|_| serde_json::json!({"ok": false})),
    ))
}

pub async fn calculate_orders_route(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<CalculateErrorResponse>)> {
    let principal = authorize_calculate_admin(&state, &headers).await?;
    let key = principal_owner_key(&principal);
    match method {
        Method::GET => {
            let templates = state
                .calculate_orders
                .list(&key)
                .await
                .map_err(store_error)?;
            Ok(Json(serde_json::json!({
                "ok": true,
                "templates": templates,
            })))
        }
        Method::POST => {
            let template: CalculateOrderTemplate = serde_json::from_slice(&body)
                .map_err(|_| bad_request("invalid_json", "invalid json"))?;
            let saved = state
                .calculate_orders
                .upsert(&key, template)
                .await
                .map_err(calculate_order_error)?;
            Ok(Json(serde_json::json!({
                "ok": true,
                "template": saved,
            })))
        }
        _ => Err(method_not_allowed()),
    }
}

pub async fn calculate_order_delete_route(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<CalculateErrorResponse>)> {
    if method != Method::POST {
        return Err(method_not_allowed());
    }
    let principal = authorize_calculate_admin(&state, &headers).await?;
    let key = principal_owner_key(&principal);
    let request: CalculateOrderDeleteRequest =
        serde_json::from_slice(&body).map_err(|_| bad_request("invalid_json", "invalid json"))?;
    if request.id.trim().is_empty() {
        return Err(bad_request("invalid_input", "id kerak"));
    }
    state
        .calculate_orders
        .delete(&key, &request.id)
        .await
        .map_err(store_error)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn authenticated_principal(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Principal, (StatusCode, Json<CalculateErrorResponse>)> {
    let token = bearer_token(headers).ok_or_else(unauthorized)?;
    state.sessions.get(&token).await.map_err(|_| unauthorized())
}

async fn authorize_calculate_admin(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Principal, (StatusCode, Json<CalculateErrorResponse>)> {
    let principal = authenticated_principal(state, headers).await?;
    let can_admin = state
        .admin
        .principal_has_capability(&principal, Capability::AdminAccess)
        .await;
    let can_production_map = state
        .admin
        .principal_has_capability(&principal, Capability::ProductionMapManage)
        .await;
    if !can_admin && !can_production_map {
        return Err(forbidden());
    }
    Ok(principal)
}

fn principal_owner_key(principal: &Principal) -> String {
    let role = match principal.role {
        PrincipalRole::Supplier => "supplier",
        PrincipalRole::Werka => "werka",
        PrincipalRole::Customer => "customer",
        PrincipalRole::Admin => "admin",
    };
    owner_key(role, &principal.ref_)
}

fn calculate_order_error(error: CalculateOrderError) -> (StatusCode, Json<CalculateErrorResponse>) {
    match error {
        CalculateOrderError::InvalidInput(detail) => bad_request("invalid_input", detail),
        CalculateOrderError::StoreFailed => store_error(CalculateOrderError::StoreFailed),
    }
}

fn store_error(_: CalculateOrderError) -> (StatusCode, Json<CalculateErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(CalculateErrorResponse::new("store_failed", "store failed")),
    )
}

fn unauthorized() -> (StatusCode, Json<CalculateErrorResponse>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(CalculateErrorResponse::new("unauthorized", "unauthorized")),
    )
}

fn forbidden() -> (StatusCode, Json<CalculateErrorResponse>) {
    (
        StatusCode::FORBIDDEN,
        Json(CalculateErrorResponse::new("forbidden", "forbidden")),
    )
}

fn bad_request(
    error: &'static str,
    detail: impl Into<String>,
) -> (StatusCode, Json<CalculateErrorResponse>) {
    (
        StatusCode::BAD_REQUEST,
        Json(CalculateErrorResponse::new(error, detail)),
    )
}

fn method_not_allowed() -> (StatusCode, Json<CalculateErrorResponse>) {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(CalculateErrorResponse::new(
            "method_not_allowed",
            "method not allowed",
        )),
    )
}

#[derive(Debug, Serialize)]
pub struct CalculateErrorResponse {
    pub ok: bool,
    pub error: &'static str,
    pub detail: String,
}

#[derive(Debug, serde::Deserialize)]
struct CalculateOrderDeleteRequest {
    #[serde(default)]
    id: String,
}

impl CalculateErrorResponse {
    fn new(error: &'static str, detail: impl Into<String>) -> Self {
        Self {
            ok: false,
            error,
            detail: detail.into(),
        }
    }
}
