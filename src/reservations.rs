use crate::{
    auth::AuthUser,
    errors::ApiError,
    models::{AuditEvent, CreateReservation, Reservation, ReservationListQuery, TransitionRequest},
    state::AppState,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

const ALL_RESERVATIONS_SELECT: &str = "
    SELECT r.id, r.equipment_id, r.user_id, r.starts_at, r.ends_at, r.purpose, r.status,
           r.version, r.created_at, r.updated_at,
           e.name AS equipment_name, e.asset_tag, u.display_name AS requester_name
    FROM reservations r
    JOIN equipment e ON e.id = r.equipment_id
    JOIN users u ON u.id = r.user_id
    ORDER BY r.starts_at DESC
";
const USER_RESERVATIONS_SELECT: &str = "
    SELECT r.id, r.equipment_id, r.user_id, r.starts_at, r.ends_at, r.purpose, r.status,
           r.version, r.created_at, r.updated_at,
           e.name AS equipment_name, e.asset_tag, u.display_name AS requester_name
    FROM reservations r
    JOIN equipment e ON e.id = r.equipment_id
    JOIN users u ON u.id = r.user_id
    WHERE r.user_id = $1
    ORDER BY r.starts_at DESC
";

pub async fn list(
    State(state): State<AppState>,
    actor: AuthUser,
    Query(query): Query<ReservationListQuery>,
) -> Result<Json<Vec<Reservation>>, ApiError> {
    if query.scope.as_deref() == Some("all") {
        actor.require_admin()?;
    }
    let rows = if query.scope.as_deref() == Some("all") {
        sqlx::query_as::<_, Reservation>(ALL_RESERVATIONS_SELECT)
            .fetch_all(&state.pool)
            .await?
    } else {
        sqlx::query_as::<_, Reservation>(USER_RESERVATIONS_SELECT)
            .bind(actor.id)
            .fetch_all(&state.pool)
            .await?
    };
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    actor: AuthUser,
    headers: HeaderMap,
    Json(input): Json<CreateReservation>,
) -> Result<(StatusCode, Json<Reservation>), ApiError> {
    let request_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| {
            (8..=100).contains(&value.len()) && value.chars().all(|ch| ch.is_ascii_graphic())
        })
        .ok_or_else(|| {
            ApiError::bad_request(
                "idempotency_key_required",
                "Idempotency-Key ヘッダーに8〜100文字の英数字キーを指定してください。",
            )
        })?;
    let now = Utc::now();
    let purpose = input.purpose.trim();
    if input.starts_at <= now || input.starts_at >= input.ends_at {
        return Err(ApiError::validation(
            "開始時刻は現在より後、終了時刻は開始時刻より後にしてください。",
        ));
    }
    if input.ends_at - input.starts_at > Duration::days(30) {
        return Err(ApiError::validation(
            "予約期間は30日以内で指定してください。",
        ));
    }
    if !(3..=280).contains(&purpose.chars().count()) {
        return Err(ApiError::validation(
            "利用目的は3〜280文字で入力してください。",
        ));
    }

    let fingerprint = format!(
        "{}|{}|{}|{}",
        input.equipment_id,
        input.starts_at.to_rfc3339(),
        input.ends_at.to_rfc3339(),
        purpose
    );
    let mut tx = state.pool.begin().await?;
    let inserted = sqlx::query_scalar::<_, String>(
        "INSERT INTO idempotency_keys (user_id, request_key, request_fingerprint)
         VALUES ($1, $2, $3)
         ON CONFLICT (user_id, request_key) DO NOTHING
         RETURNING request_key",
    )
    .bind(actor.id)
    .bind(request_key)
    .bind(&fingerprint)
    .fetch_optional(&mut *tx)
    .await?;

    if inserted.is_none() {
        let existing = sqlx::query_as::<_, (String, Option<Uuid>)>(
            "SELECT request_fingerprint, reservation_id FROM idempotency_keys
             WHERE user_id = $1 AND request_key = $2",
        )
        .bind(actor.id)
        .bind(request_key)
        .fetch_one(&mut *tx)
        .await?;
        if existing.0 != fingerprint {
            return Err(ApiError::conflict(
                "idempotency_key_reused",
                "同じ Idempotency-Key が別の予約内容に使われています。",
            ));
        }
        let reservation_id = existing.1.ok_or_else(|| {
            ApiError::conflict(
                "request_in_progress",
                "同じ予約要求を処理しています。少し待ってから再試行してください。",
            )
        })?;
        let reservation = find_reservation(&mut tx, reservation_id).await?;
        tx.commit().await?;
        return Ok((StatusCode::OK, Json(reservation)));
    }

    let equipment_status =
        sqlx::query_scalar::<_, String>("SELECT status FROM equipment WHERE id = $1 FOR SHARE")
            .bind(input.equipment_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ApiError::not_found("備品"))?;
    if equipment_status != "active" {
        return Err(ApiError::conflict(
            "equipment_unavailable",
            "この備品は現在利用できません。",
        ));
    }

    let reservation = sqlx::query_as::<_, Reservation>(
        "INSERT INTO reservations (equipment_id, user_id, starts_at, ends_at, purpose)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, equipment_id, user_id, starts_at, ends_at, purpose, status,
                   version, created_at, updated_at,
                   ''::text AS equipment_name, ''::text AS asset_tag, ''::text AS requester_name",
    )
    .bind(input.equipment_id)
    .bind(actor.id)
    .bind(input.starts_at)
    .bind(input.ends_at)
    .bind(purpose)
    .fetch_one(&mut *tx)
    .await;

    let reservation_id = match reservation {
        Ok(row) => row.id,
        Err(error) => return Err(ApiError::database(error)),
    };

    sqlx::query(
        "UPDATE idempotency_keys SET reservation_id = $3 WHERE user_id = $1 AND request_key = $2",
    )
    .bind(actor.id)
    .bind(request_key)
    .bind(reservation_id)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        reservation_id,
        actor.id,
        "reservation.created",
        None,
        Some("pending"),
        json!({"equipment_id": input.equipment_id}),
    )
    .await?;
    let reservation = find_reservation(&mut tx, reservation_id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(reservation)))
}

pub async fn cancel(
    State(state): State<AppState>,
    actor: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<TransitionRequest>,
) -> Result<Json<Reservation>, ApiError> {
    transition(state, actor, id, "cancelled", input.note, false).await
}

pub async fn approve(
    State(state): State<AppState>,
    actor: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<TransitionRequest>,
) -> Result<Json<Reservation>, ApiError> {
    transition(state, actor, id, "approved", input.note, true).await
}

pub async fn reject(
    State(state): State<AppState>,
    actor: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<TransitionRequest>,
) -> Result<Json<Reservation>, ApiError> {
    transition(state, actor, id, "rejected", input.note, true).await
}

pub async fn checkout(
    State(state): State<AppState>,
    actor: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<TransitionRequest>,
) -> Result<Json<Reservation>, ApiError> {
    transition(state, actor, id, "checked_out", input.note, true).await
}

pub async fn return_item(
    State(state): State<AppState>,
    actor: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<TransitionRequest>,
) -> Result<Json<Reservation>, ApiError> {
    transition(state, actor, id, "returned", input.note, true).await
}

async fn transition(
    state: AppState,
    actor: AuthUser,
    id: Uuid,
    target: &'static str,
    note: Option<String>,
    admin_only: bool,
) -> Result<Json<Reservation>, ApiError> {
    if admin_only {
        actor.require_admin()?;
    }
    let note = note.unwrap_or_default();
    if note.chars().count() > 280 {
        return Err(ApiError::validation(
            "メモは280文字以内で入力してください。",
        ));
    }
    let mut tx = state.pool.begin().await?;
    let current = sqlx::query_as::<_, (String, Uuid)>(
        "SELECT status, user_id FROM reservations WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| ApiError::not_found("予約"))?;
    let (from, owner_id) = current;
    if target == "cancelled" && owner_id != actor.id && !actor.is_admin() {
        return Err(ApiError::forbidden());
    }
    if !transition_allowed(&from, target) {
        return Err(ApiError::conflict(
            "invalid_transition",
            format!("予約状態「{from}」から「{target}」へ変更できません。"),
        ));
    }

    sqlx::query(
        "UPDATE reservations SET status = $2, version = version + 1, updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(target)
    .execute(&mut *tx)
    .await?;
    let details: Value = if note.trim().is_empty() {
        json!({})
    } else {
        json!({"note": note.trim()})
    };
    audit(
        &mut tx,
        id,
        actor.id,
        "reservation.status_changed",
        Some(&from),
        Some(target),
        details,
    )
    .await?;
    let reservation = find_reservation(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(reservation))
}

fn transition_allowed(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("pending", "approved" | "rejected" | "cancelled")
            | ("approved", "checked_out" | "cancelled")
            | ("checked_out", "returned")
    )
}

async fn find_reservation(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> Result<Reservation, ApiError> {
    sqlx::query_as::<_, Reservation>(
        "SELECT r.id, r.equipment_id, r.user_id, r.starts_at, r.ends_at, r.purpose, r.status,
                r.version, r.created_at, r.updated_at,
                e.name AS equipment_name, e.asset_tag, u.display_name AS requester_name
         FROM reservations r
         JOIN equipment e ON e.id = r.equipment_id
         JOIN users u ON u.id = r.user_id
         WHERE r.id = $1",
    )
    .bind(id)
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::from)
}

pub async fn audit_log(
    State(state): State<AppState>,
    actor: AuthUser,
) -> Result<Json<Vec<AuditEvent>>, ApiError> {
    actor.require_admin()?;
    let events = sqlx::query_as::<_, AuditEvent>(
        "SELECT a.id, a.entity_type, a.entity_id, a.actor_user_id,
                u.display_name AS actor_name, a.action, a.from_state, a.to_state, a.occurred_at
         FROM audit_events a JOIN users u ON u.id = a.actor_user_id
         ORDER BY a.occurred_at DESC, a.id DESC LIMIT 100",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(events))
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    entity_id: Uuid,
    actor_id: Uuid,
    action: &str,
    from_state: Option<&str>,
    to_state: Option<&str>,
    details: Value,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO audit_events
         (entity_type, entity_id, actor_user_id, action, from_state, to_state, details)
         VALUES ('reservation', $1, $2, $3, $4, $5, $6)",
    )
    .bind(entity_id)
    .bind(actor_id)
    .bind(action)
    .bind(from_state)
    .bind(to_state)
    .bind(details)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::transition_allowed;

    #[test]
    fn reservation_lifecycle_accepts_only_named_forward_transitions() {
        assert!(transition_allowed("pending", "approved"));
        assert!(transition_allowed("pending", "rejected"));
        assert!(transition_allowed("pending", "cancelled"));
        assert!(transition_allowed("approved", "checked_out"));
        assert!(transition_allowed("approved", "cancelled"));
        assert!(transition_allowed("checked_out", "returned"));
        assert!(!transition_allowed("pending", "returned"));
        assert!(!transition_allowed("returned", "approved"));
        assert!(!transition_allowed("rejected", "cancelled"));
    }
}
