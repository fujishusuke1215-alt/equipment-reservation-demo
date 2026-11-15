use crate::{errors::ApiError, models::Person, state::AppState};
use axum::{
    Json,
    extract::FromRequestParts,
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{COOKIE, SET_COOKIE},
        request::Parts,
    },
    response::IntoResponse,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

pub const SESSION_COOKIE: &str = "gear_session";

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub id: Uuid,
    pub name: String,
    pub role: String,
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    pub fn require_admin(&self) -> Result<(), ApiError> {
        if self.is_admin() {
            Ok(())
        } else {
            Err(ApiError::forbidden())
        }
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        load_session(&state.pool, &parts.headers)
            .await?
            .ok_or_else(ApiError::unauthorized)
    }
}

pub async fn load_session(
    pool: &sqlx::PgPool,
    headers: &HeaderMap,
) -> Result<Option<AuthUser>, ApiError> {
    let Some(session_id) = session_id(headers) else {
        return Ok(None);
    };
    let row = sqlx::query(
        "SELECT u.id, u.display_name, u.role
         FROM sessions s
         JOIN users u ON u.id = s.user_id
         WHERE s.id = $1 AND s.expires_at > now()",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| AuthUser {
        id: row.get("id"),
        name: row.get("display_name"),
        role: row.get("role"),
    }))
}

fn session_id(headers: &HeaderMap) -> Option<Uuid> {
    let cookie_header = headers.get(COOKIE)?.to_str().ok()?;
    cookie_header.split(';').find_map(|cookie| {
        let (name, value) = cookie.trim().split_once('=')?;
        (name == SESSION_COOKIE)
            .then(|| Uuid::parse_str(value).ok())
            .flatten()
    })
}

#[derive(Deserialize)]
pub struct DevLoginRequest {
    role: String,
}

#[derive(Serialize)]
pub struct SessionResponse {
    pub user: Option<Person>,
}

pub async fn current_session(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SessionResponse>, ApiError> {
    let user = load_session(&state.pool, &headers)
        .await?
        .map(|session| Person {
            id: session.id,
            display_name: session.name,
            role: session.role,
        });
    Ok(Json(SessionResponse { user }))
}

pub async fn dev_login(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(request): Json<DevLoginRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if !state.demo_auth_enabled {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "demo_auth_disabled",
            "Not found",
            "開発用ログインは無効です。",
        ));
    }
    let email = match request.role.as_str() {
        "member" => "member@demo.local",
        "admin" => "admin@demo.local",
        _ => {
            return Err(ApiError::bad_request(
                "invalid_role",
                "role は member または admin を指定してください。",
            ));
        }
    };
    let person =
        sqlx::query_as::<_, Person>("SELECT id, display_name, role FROM users WHERE email = $1")
            .bind(email)
            .fetch_one(&state.pool)
            .await?;

    let session_id = Uuid::new_v4();
    let expires_at = Utc::now() + Duration::hours(12);
    sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(session_id)
        .bind(person.id)
        .bind(expires_at)
        .execute(&state.pool)
        .await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&format!(
            "{SESSION_COOKIE}={session_id}; HttpOnly; SameSite=Lax; Path=/; Max-Age=43200"
        ))
        .expect("generated session cookie is valid"),
    );
    Ok((headers, Json(SessionResponse { user: Some(person) })))
}

pub async fn logout(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    if let Some(session_id) = session_id(&headers) {
        sqlx::query("DELETE FROM sessions WHERE id = $1")
            .bind(session_id)
            .execute(&state.pool)
            .await?;
    }
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        SET_COOKIE,
        HeaderValue::from_static("gear_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"),
    );
    Ok((response_headers, StatusCode::NO_CONTENT))
}
