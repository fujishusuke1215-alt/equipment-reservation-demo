use crate::{
    auth::AuthUser,
    errors::ApiError,
    models::{BusySlot, CreateEquipment, Equipment, PeriodQuery, UpdateEquipment},
    state::AppState,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{Duration, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Serialize)]
pub struct AvailabilityResponse {
    equipment_id: Uuid,
    from: chrono::DateTime<Utc>,
    to: chrono::DateTime<Utc>,
    busy_slots: Vec<BusySlot>,
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Equipment>>, ApiError> {
    let rows = sqlx::query_as::<_, Equipment>(
        "SELECT id, asset_tag, name, category, description, location, status, created_at, updated_at
         FROM equipment ORDER BY CASE status WHEN 'active' THEN 0 WHEN 'maintenance' THEN 1 ELSE 2 END, name",
    ).fetch_all(&state.pool).await?;
    Ok(Json(rows))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Equipment>, ApiError> {
    Ok(Json(find_equipment(&state.pool, id).await?))
}

pub async fn availability(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<PeriodQuery>,
) -> Result<Json<AvailabilityResponse>, ApiError> {
    find_equipment(&state.pool, id).await?;
    let (from, to) = validate_period(query.from, query.to)?;
    let busy_slots = sqlx::query_as::<_, BusySlot>(
        "SELECT starts_at, ends_at FROM reservations
         WHERE equipment_id = $1 AND status IN ('pending', 'approved', 'checked_out')
           AND starts_at < $3 AND ends_at > $2 ORDER BY starts_at",
    )
    .bind(id)
    .bind(from)
    .bind(to)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(AvailabilityResponse {
        equipment_id: id,
        from,
        to,
        busy_slots,
    }))
}

pub async fn create(
    State(state): State<AppState>,
    actor: AuthUser,
    Json(input): Json<CreateEquipment>,
) -> Result<(StatusCode, Json<Equipment>), ApiError> {
    actor.require_admin()?;
    validate_equipment(
        &input.asset_tag,
        &input.name,
        &input.category,
        &input.description,
        &input.location,
    )?;
    let mut tx = state.pool.begin().await?;
    let item = sqlx::query_as::<_, Equipment>(
        "INSERT INTO equipment (asset_tag, name, category, description, location)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, asset_tag, name, category, description, location, status, created_at, updated_at",
    ).bind(input.asset_tag.trim().to_uppercase()).bind(input.name.trim())
        .bind(input.category.trim()).bind(input.description.trim()).bind(input.location.trim())
        .fetch_one(&mut *tx).await?;
    audit_equipment(
        &mut tx,
        item.id,
        actor.id,
        "equipment.created",
        None,
        Some("active"),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(item)))
}

pub async fn update(
    State(state): State<AppState>,
    actor: AuthUser,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateEquipment>,
) -> Result<Json<Equipment>, ApiError> {
    actor.require_admin()?;
    validate_equipment(
        &input.asset_tag,
        &input.name,
        &input.category,
        &input.description,
        &input.location,
    )?;
    if !matches!(input.status.as_str(), "active" | "maintenance" | "retired") {
        return Err(ApiError::validation(
            "status は active、maintenance、retired のいずれかを指定してください。",
        ));
    }
    let mut tx = state.pool.begin().await?;
    let old = sqlx::query_as::<_, Equipment>(
        "SELECT id, asset_tag, name, category, description, location, status, created_at, updated_at
         FROM equipment WHERE id = $1 FOR UPDATE",
    ).bind(id).fetch_optional(&mut *tx).await?.ok_or_else(|| ApiError::not_found("備品"))?;
    if input.status != "active" {
        let open_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM reservations WHERE equipment_id = $1 AND status IN ('pending', 'approved', 'checked_out')",
        ).bind(id).fetch_one(&mut *tx).await?;
        if open_count > 0 {
            return Err(ApiError::conflict(
                "equipment_has_open_reservations",
                "未完了の予約がある備品は利用停止にできません。先に予約を処理してください。",
            ));
        }
    }
    let item = sqlx::query_as::<_, Equipment>(
        "UPDATE equipment SET asset_tag = $2, name = $3, category = $4, description = $5,
             location = $6, status = $7, updated_at = now() WHERE id = $1
         RETURNING id, asset_tag, name, category, description, location, status, created_at, updated_at",
    ).bind(id).bind(input.asset_tag.trim().to_uppercase()).bind(input.name.trim())
        .bind(input.category.trim()).bind(input.description.trim()).bind(input.location.trim()).bind(&input.status)
        .fetch_one(&mut *tx).await?;
    audit_equipment(
        &mut tx,
        id,
        actor.id,
        "equipment.updated",
        Some(&old.status),
        Some(&item.status),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(item))
}

async fn find_equipment(pool: &PgPool, id: Uuid) -> Result<Equipment, ApiError> {
    sqlx::query_as::<_, Equipment>(
        "SELECT id, asset_tag, name, category, description, location, status, created_at, updated_at FROM equipment WHERE id = $1",
    ).bind(id).fetch_optional(pool).await?.ok_or_else(|| ApiError::not_found("備品"))
}

fn validate_period(
    from: Option<chrono::DateTime<Utc>>,
    to: Option<chrono::DateTime<Utc>>,
) -> Result<(chrono::DateTime<Utc>, chrono::DateTime<Utc>), ApiError> {
    let from = from.unwrap_or_else(Utc::now);
    let to = to.unwrap_or(from + Duration::days(31));
    if from >= to || to - from > Duration::days(31) {
        return Err(ApiError::validation(
            "期間は開始時刻より後、かつ31日以内で指定してください。",
        ));
    }
    Ok((from, to))
}

fn validate_equipment(
    tag: &str,
    name: &str,
    category: &str,
    description: &str,
    location: &str,
) -> Result<(), ApiError> {
    let tag = tag.trim();
    let valid_tag = (3..=32).contains(&tag.len())
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if !valid_tag {
        return Err(ApiError::validation(
            "備品番号は英数字・ハイフン・アンダースコアを使って3〜32文字で入力してください。",
        ));
    }
    if !(2..=120).contains(&name.trim().chars().count())
        || !(2..=60).contains(&category.trim().chars().count())
        || !(2..=100).contains(&location.trim().chars().count())
        || description.chars().count() > 1000
    {
        return Err(ApiError::validation(
            "備品名、カテゴリ、保管場所、説明の文字数を確認してください。",
        ));
    }
    Ok(())
}

async fn audit_equipment(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    actor: Uuid,
    action: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_events (entity_type, entity_id, actor_user_id, action, from_state, to_state)
         VALUES ('equipment', $1, $2, $3, $4, $5)",
    ).bind(id).bind(actor).bind(action).bind(from).bind(to).execute(&mut **tx).await?;
    Ok(())
}
