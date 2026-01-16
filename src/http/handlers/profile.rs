use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::app::AppState;
use crate::core::auth::models::PrincipalRole;
use crate::http::handlers::auth::{ErrorResponse, bearer_token};

#[derive(Debug, Deserialize)]
pub struct AvatarViewQuery {
    token: Option<String>,
}

pub async fn avatar_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AvatarViewQuery>,
) -> Response {
    let token = match query
        .token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(token) => token.to_string(),
        None => match bearer_token(&headers) {
            Some(token) => token,
            None => return unauthorized().into_response(),
        },
    };

    let Ok(principal) = state.sessions.get(&token).await else {
        return unauthorized().into_response();
    };
    if principal.role != PrincipalRole::Supplier {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(ErrorResponse { error: "forbidden" }),
        )
            .into_response();
    }

    match state.profiles.download_avatar(principal).await {
        Ok(Some(file)) => {
            let mut response = Body::from(file.body).into_response();
            if !file.content_type.trim().is_empty() {
                response.headers_mut().insert(
                    header::CONTENT_TYPE,
                    file.content_type
                        .parse()
                        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
                );
            }
            response
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(ErrorResponse {
                error: "avatar fetch failed",
            }),
        )
            .into_response(),
    }
}

fn unauthorized() -> (StatusCode, axum::Json<ErrorResponse>) {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(ErrorResponse {
            error: "unauthorized",
        }),
    )
}
