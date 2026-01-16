use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode};

use crate::app::AppState;
use crate::core::werka::models::{ConfirmReceiptRequest, DispatchRecord};
use crate::http::handlers::auth::ErrorResponse;
use crate::http::handlers::werka::authz::{authorize, require_werka};

pub async fn confirm(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<DispatchRecord>, (StatusCode, Json<ErrorResponse>)> {
    if method != Method::POST {
        return Err((
            StatusCode::METHOD_NOT_ALLOWED,
            Json(ErrorResponse {
                error: "method not allowed",
            }),
        ));
    }
    let principal = authorize(&state, &headers).await?;
    require_werka(&principal)?;

    let request: ConfirmReceiptRequest = serde_json::from_slice(&body).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "invalid json",
            }),
        )
    })?;

    match state
        .werka
        .confirm_receipt(
            &request.receipt_id,
            request.accepted_qty,
            request.returned_qty,
            &request.return_reason,
            &request.return_comment,
        )
        .await
    {
        Ok(Some(record)) => Ok(Json(record)),
        Ok(None) | Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "receipt confirm failed",
            }),
        )),
    }
}
