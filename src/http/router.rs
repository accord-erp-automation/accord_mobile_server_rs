use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use tower_http::trace::TraceLayer;

use crate::app::AppState;
use crate::http::handlers::{auth, profile};

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/mobile/auth/login", post(auth::login))
        .route("/v1/mobile/auth/logout", post(auth::logout))
        .route("/v1/mobile/me", get(auth::me))
        .route("/v1/mobile/profile/avatar/view", get(profile::avatar_view))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
}

async fn healthz(State(state): State<AppState>) -> Json<HealthResponse> {
    let _ = state.config.bind_addr;

    Json(HealthResponse { ok: true })
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use tower::ServiceExt;

    use super::build_router;
    use crate::app::AppState;
    use crate::config::AppConfig;
    use crate::core::auth::models::{Principal, PrincipalRole};

    fn test_state() -> AppState {
        AppState::new(AppConfig {
            bind_addr: "127.0.0.1:8081".parse().expect("addr"),
            erp_url: String::new(),
            erp_api_key: String::new(),
            erp_api_secret: String::new(),
            erp_timeout: std::time::Duration::from_secs(15),
            session_store_path: "data/mobile_sessions.json".into(),
            admin_supplier_store_path: "data/mobile_admin_suppliers.json".into(),
            session_ttl_seconds: Some(30 * 24 * 60 * 60),
            supplier_prefix: "10".to_string(),
            werka_prefix: "20".to_string(),
            werka_code: "20ABCDEF1234".to_string(),
            werka_name: "Werka".to_string(),
            admin_phone: "+998880000000".to_string(),
            admin_name: "Admin".to_string(),
            admin_code: "19621978".to_string(),
        })
    }

    #[tokio::test]
    async fn me_route_matches_go_contract() {
        let app = build_router(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/mobile/me")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn auth_me_route_is_not_registered() {
        let app = build_router(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/mobile/auth/me")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn avatar_view_requires_auth() {
        let app = build_router(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/mobile/profile/avatar/view")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn avatar_view_forbids_non_supplier() {
        let state = test_state();
        let token = state
            .sessions
            .create(Principal {
                role: PrincipalRole::Customer,
                display_name: "Customer".to_string(),
                legal_name: "Customer".to_string(),
                ref_: "CUST-001".to_string(),
                phone: "+998901234567".to_string(),
                avatar_url: String::new(),
            })
            .await
            .expect("session");
        let app = build_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/mobile/profile/avatar/view")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
