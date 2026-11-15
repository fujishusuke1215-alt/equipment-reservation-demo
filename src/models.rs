use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, FromRow)]
pub struct Person {
    pub id: Uuid,
    pub display_name: String,
    pub role: String,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct Equipment {
    pub id: Uuid,
    pub asset_tag: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub location: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct Reservation {
    pub id: Uuid,
    pub equipment_id: Uuid,
    pub user_id: Uuid,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub purpose: String,
    pub status: String,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub equipment_name: String,
    pub asset_tag: String,
    pub requester_name: String,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct BusySlot {
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct AuditEvent {
    pub id: i64,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub actor_user_id: Uuid,
    pub actor_name: String,
    pub action: String,
    pub from_state: Option<String>,
    pub to_state: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct PeriodQuery {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub struct ReservationListQuery {
    pub scope: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateReservation {
    pub equipment_id: Uuid,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub purpose: String,
}

#[derive(Deserialize)]
pub struct CreateEquipment {
    pub asset_tag: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub location: String,
}

#[derive(Deserialize)]
pub struct UpdateEquipment {
    pub asset_tag: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub location: String,
    pub status: String,
}

#[derive(Deserialize)]
pub struct TransitionRequest {
    pub note: Option<String>,
}
