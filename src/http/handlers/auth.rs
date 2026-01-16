use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde::Serialize;

use crate::app::AppState;
use crate::core::auth::models::{LoginRequest, LoginResponse, Principal};
use crate::core::auth::service::AuthError;

pub async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ErrorResponse>)> {
    let principal = state
        .auth
        .login(request.phone.trim(), request.code.trim())
        .await
        .map_err(login_error)?;
    let token = state
        .sessions
        .create(principal.clone())
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "session create failed",
                }),
            )
        })?;

    Ok(Json(LoginResponse {
        token,
        profile: principal,
        werka_home: None,
    }))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OkResponse>, (StatusCode, Json<ErrorResponse>)> {
    let token = bearer_token(&headers).ok_or_else(unauthorized)?;
    state.sessions.delete(&token).await;

    Ok(Json(OkResponse { ok: true }))
}

pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Principal>, (StatusCode, Json<ErrorResponse>)> {
    let token = bearer_token(&headers).ok_or_else(unauthorized)?;
    let principal = state
        .sessions
        .get(&token)
        .await
        .map_err(|_| unauthorized())?;
    state.sessions.update(&token, principal.clone()).await;

    Ok(Json(principal))
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let token = raw.strip_prefix("Bearer ")?.trim();

    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

fn unauthorized() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse {
            error: "unauthorized",
        }),
    )
}

fn login_error(error: AuthError) -> (StatusCode, Json<ErrorResponse>) {
    match error {
        AuthError::InvalidCredentials | AuthError::InvalidRole => (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "invalid credentials",
            }),
        ),
    }
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: &'static str,
}

#[derive(Serialize)]
pub struct OkResponse {
    pub ok: bool,
}

#[allow(dead_code)]
fn _login_response_contract(_response: LoginResponse) {}
