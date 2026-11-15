use crate::{auth, equipment, errors::normalize_error_response, reservations, state::AppState};
use axum::{
    Json, Router,
    http::{HeaderValue, header},
    middleware::map_response,
    response::IntoResponse,
    routing::{get, post},
};
use serde::Serialize;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/openapi.yaml", get(openapi))
        .route("/api/auth/session", get(auth::current_session))
        .route("/api/auth/dev-login", post(auth::dev_login))
        .route("/api/auth/logout", post(auth::logout))
        .route(
            "/api/equipment",
            get(equipment::list).post(equipment::create),
        )
        .route(
            "/api/equipment/{id}",
            get(equipment::get_one).patch(equipment::update),
        )
        .route(
            "/api/equipment/{id}/availability",
            get(equipment::availability),
        )
        .route(
            "/api/reservations",
            get(reservations::list).post(reservations::create),
        )
        .route("/api/reservations/{id}/cancel", post(reservations::cancel))
        .route(
            "/api/reservations/{id}/approve",
            post(reservations::approve),
        )
        .route("/api/reservations/{id}/reject", post(reservations::reject))
        .route(
            "/api/reservations/{id}/checkout",
            post(reservations::checkout),
        )
        .route(
            "/api/reservations/{id}/return",
            post(reservations::return_item),
        )
        .route("/api/audit", get(reservations::audit_log))
        .with_state(state)
        .layer(map_response(normalize_error_response))
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn openapi() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/yaml; charset=utf-8"),
        )],
        include_str!("../openapi.yaml"),
    )
}
