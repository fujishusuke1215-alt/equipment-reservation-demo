use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use chrono::{Duration, Utc};
use equipment_reservation_api::{AppState, migrate, router};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::env;
use tokio::sync::OnceCell;
use tower::ServiceExt;
use uuid::Uuid;

static MIGRATED: OnceCell<()> = OnceCell::const_new();

async fn pool() -> PgPool {
    let url = env::var("TEST_DATABASE_URL")
        .or_else(|_| env::var("DATABASE_URL"))
        .expect("TEST_DATABASE_URL or DATABASE_URL must point to a disposable PostgreSQL database");
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&url)
        .await
        .expect("connect to test database");
    MIGRATED
        .get_or_init(|| async {
            migrate(&pool).await.expect("run test migrations");
        })
        .await;
    pool
}

#[derive(Clone)]
struct Actor {
    id: Uuid,
    cookie: String,
}

async fn actor(pool: &PgPool, role: &'static str) -> Actor {
    let id = Uuid::new_v4();
    let session = Uuid::new_v4();
    let email = format!("{}@test.local", Uuid::new_v4());
    sqlx::query("INSERT INTO users (id, email, display_name, role) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind(email)
        .bind(format!("Test {role}"))
        .bind(role)
        .execute(pool)
        .await
        .expect("insert test user");
    sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at) VALUES ($1, $2, now() + interval '1 hour')",
    )
    .bind(session)
    .bind(id)
    .execute(pool)
    .await
    .expect("insert test session");
    Actor {
        id,
        cookie: format!("gear_session={session}"),
    }
}

async fn equipment(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    let tag = format!("T-{}", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO equipment (id, asset_tag, name, category, location) VALUES ($1, $2, 'Test camera', 'Video', 'Test room')",
    ).bind(id).bind(tag).execute(pool).await.expect("insert test equipment");
    id
}

fn app(pool: PgPool) -> Router {
    router(AppState {
        pool,
        demo_auth_enabled: true,
    })
}

fn json_request(
    method: &str,
    uri: String,
    actor: &Actor,
    body: Value,
    key: Option<String>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Cookie", &actor.cookie)
        .header("Content-Type", "application/json");
    if let Some(key) = key {
        builder = builder.header("Idempotency-Key", key);
    }
    builder
        .body(Body::from(body.to_string()))
        .expect("build request")
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response");
    serde_json::from_slice(&bytes).expect("decode JSON response")
}

fn reservation_input(equipment_id: Uuid) -> (Value, chrono::DateTime<Utc>, chrono::DateTime<Utc>) {
    let start = Utc::now() + Duration::days(2);
    let end = start + Duration::hours(3);
    (
        json!({
            "equipment_id": equipment_id,
            "starts_at": start,
            "ends_at": end,
            "purpose": "Customer workshop"
        }),
        start,
        end,
    )
}

#[tokio::test]
async fn creates_and_cancels_a_reservation_with_audit_events() {
    let pool = pool().await;
    let member = actor(&pool, "member").await;
    let equipment_id = equipment(&pool).await;
    let (payload, _, _) = reservation_input(equipment_id);

    let response = app(pool.clone())
        .oneshot(json_request(
            "POST",
            "/api/reservations".to_owned(),
            &member,
            payload,
            Some(Uuid::new_v4().to_string()),
        ))
        .await
        .expect("create reservation");
    assert_eq!(response.status(), StatusCode::CREATED);
    let created = body_json(response).await;
    assert_eq!(created["status"], "pending");
    let reservation_id = created["id"].as_str().expect("reservation id");

    let response = app(pool.clone())
        .oneshot(json_request(
            "POST",
            format!("/api/reservations/{reservation_id}/cancel"),
            &member,
            json!({"note": "Plans changed"}),
            None,
        ))
        .await
        .expect("cancel reservation");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["status"], "cancelled");

    let events: Vec<(Uuid, String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT actor_user_id, action, occurred_at FROM audit_events
         WHERE entity_type = 'reservation' AND entity_id = $1 ORDER BY id",
    )
    .bind(Uuid::parse_str(reservation_id).unwrap())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, member.id);
    assert_eq!(events[1].0, member.id);
    assert_eq!(events[0].1, "reservation.created");
    assert_eq!(events[1].1, "reservation.status_changed");
    assert!(events[0].2 <= events[1].2);
}

#[tokio::test]
async fn rejects_overlapping_periods_and_allows_adjacent_periods() {
    let pool = pool().await;
    let first_member = actor(&pool, "member").await;
    let second_member = actor(&pool, "member").await;
    let equipment_id = equipment(&pool).await;
    let (payload, start, end) = reservation_input(equipment_id);

    let response = app(pool.clone())
        .oneshot(json_request(
            "POST",
            "/api/reservations".to_owned(),
            &first_member,
            payload,
            Some(Uuid::new_v4().to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let overlap = json!({
        "equipment_id": equipment_id,
        "starts_at": start + Duration::hours(1),
        "ends_at": end + Duration::hours(1),
        "purpose": "Overlapping workshop"
    });
    let response = app(pool.clone())
        .oneshot(json_request(
            "POST",
            "/api/reservations".to_owned(),
            &second_member,
            overlap,
            Some(Uuid::new_v4().to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(body_json(response).await["code"], "reservation_overlap");

    let adjacent = json!({
        "equipment_id": equipment_id,
        "starts_at": end,
        "ends_at": end + Duration::hours(2),
        "purpose": "Next workshop"
    });
    let response = app(pool.clone())
        .oneshot(json_request(
            "POST",
            "/api/reservations".to_owned(),
            &second_member,
            adjacent,
            Some(Uuid::new_v4().to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn simultaneous_overlapping_requests_create_only_one_reservation() {
    let pool = pool().await;
    let member_a = actor(&pool, "member").await;
    let member_b = actor(&pool, "member").await;
    let equipment_id = equipment(&pool).await;
    let (payload, _, _) = reservation_input(equipment_id);
    let first = json_request(
        "POST",
        "/api/reservations".to_owned(),
        &member_a,
        payload.clone(),
        Some(Uuid::new_v4().to_string()),
    );
    let second = json_request(
        "POST",
        "/api/reservations".to_owned(),
        &member_b,
        payload,
        Some(Uuid::new_v4().to_string()),
    );
    let service = app(pool.clone());
    let (left, right) = tokio::join!(service.clone().oneshot(first), service.oneshot(second));
    let statuses = [left.unwrap().status(), right.unwrap().status()];
    assert!(
        statuses.contains(&StatusCode::CREATED),
        "one request should succeed: {statuses:?}"
    );
    assert!(
        statuses.contains(&StatusCode::CONFLICT),
        "one request should be rejected: {statuses:?}"
    );

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM reservations WHERE equipment_id = $1 AND status IN ('pending', 'approved', 'checked_out')",
    ).bind(equipment_id).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn idempotency_replays_the_original_reservation_without_duplicate_rows() {
    let pool = pool().await;
    let member = actor(&pool, "member").await;
    let equipment_id = equipment(&pool).await;
    let (payload, _, _) = reservation_input(equipment_id);
    let key = Uuid::new_v4().to_string();
    let service = app(pool.clone());
    let (left, right) = tokio::join!(
        service.clone().oneshot(json_request(
            "POST",
            "/api/reservations".to_owned(),
            &member,
            payload.clone(),
            Some(key.clone())
        )),
        service.oneshot(json_request(
            "POST",
            "/api/reservations".to_owned(),
            &member,
            payload,
            Some(key)
        )),
    );
    let left = left.unwrap();
    let right = right.unwrap();
    assert!([left.status(), right.status()].contains(&StatusCode::CREATED));
    assert!([left.status(), right.status()].contains(&StatusCode::OK));
    let left_id = body_json(left).await["id"].as_str().unwrap().to_owned();
    let right_id = body_json(right).await["id"].as_str().unwrap().to_owned();
    assert_eq!(left_id, right_id);

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM reservations WHERE id = $1")
        .bind(Uuid::parse_str(&left_id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn role_checks_and_transition_rules_are_enforced_by_the_api() {
    let pool = pool().await;
    let member = actor(&pool, "member").await;
    let admin = actor(&pool, "admin").await;
    let item_id = equipment(&pool).await;
    let (payload, _, _) = reservation_input(item_id);
    let response = app(pool.clone())
        .oneshot(json_request(
            "POST",
            "/api/reservations".to_owned(),
            &member,
            payload,
            Some(Uuid::new_v4().to_string()),
        ))
        .await
        .unwrap();
    let reservation_id = body_json(response).await["id"].as_str().unwrap().to_owned();

    let response = app(pool.clone())
        .oneshot(json_request(
            "POST",
            format!("/api/reservations/{reservation_id}/approve"),
            &member,
            json!({}),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = app(pool.clone())
        .oneshot(json_request(
            "POST",
            format!("/api/reservations/{reservation_id}/return"),
            &admin,
            json!({}),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(body_json(response).await["code"], "invalid_transition");

    let create_equipment = json!({
        "asset_tag": format!("A-{}", &Uuid::new_v4().simple().to_string()[..20]),
        "name": "Portable speaker",
        "category": "Audio",
        "description": "",
        "location": "Test room"
    });
    let response = app(pool.clone())
        .oneshot(json_request(
            "POST",
            "/api/equipment".to_owned(),
            &member,
            create_equipment.clone(),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = app(pool.clone())
        .oneshot(json_request(
            "POST",
            "/api/equipment".to_owned(),
            &admin,
            create_equipment,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = app(pool.clone())
        .oneshot(json_request(
            "GET",
            "/api/audit".to_owned(),
            &member,
            json!({}),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = app(pool)
        .oneshot(json_request(
            "GET",
            "/api/audit".to_owned(),
            &admin,
            json!({}),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_member_cannot_cancel_another_members_reservation() {
    let pool = pool().await;
    let owner = actor(&pool, "member").await;
    let stranger = actor(&pool, "member").await;
    let item_id = equipment(&pool).await;
    let (payload, _, _) = reservation_input(item_id);
    let response = app(pool.clone())
        .oneshot(json_request(
            "POST",
            "/api/reservations".to_owned(),
            &owner,
            payload,
            Some(Uuid::new_v4().to_string()),
        ))
        .await
        .unwrap();
    let reservation_id = body_json(response).await["id"].as_str().unwrap().to_owned();
    let response = app(pool)
        .oneshot(json_request(
            "POST",
            format!("/api/reservations/{reservation_id}/cancel"),
            &stranger,
            json!({}),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn equipment_cannot_be_paused_until_open_reservations_are_resolved() {
    let pool = pool().await;
    let member = actor(&pool, "member").await;
    let admin = actor(&pool, "admin").await;
    let item_id = equipment(&pool).await;
    let (payload, _, _) = reservation_input(item_id);
    let response = app(pool.clone())
        .oneshot(json_request(
            "POST",
            "/api/reservations".to_owned(),
            &member,
            payload,
            Some(Uuid::new_v4().to_string()),
        ))
        .await
        .unwrap();
    let reservation_id = body_json(response).await["id"].as_str().unwrap().to_owned();
    let update = || {
        json!({
            "asset_tag": format!("P-{}", &Uuid::new_v4().simple().to_string()[..20]),
            "name": "Test camera",
            "category": "Video",
            "description": "",
            "location": "Test room",
            "status": "maintenance"
        })
    };

    let response = app(pool.clone())
        .oneshot(json_request(
            "PATCH",
            format!("/api/equipment/{item_id}"),
            &admin,
            update(),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        body_json(response).await["code"],
        "equipment_has_open_reservations"
    );

    let response = app(pool.clone())
        .oneshot(json_request(
            "POST",
            format!("/api/reservations/{reservation_id}/cancel"),
            &admin,
            json!({}),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app(pool.clone())
        .oneshot(json_request(
            "PATCH",
            format!("/api/equipment/{item_id}"),
            &admin,
            update(),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["status"], "maintenance");
}

#[tokio::test]
async fn malformed_json_uses_the_problem_details_error_envelope() {
    let pool = pool().await;
    let member = actor(&pool, "member").await;
    let response = app(pool)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/reservations")
                .header("Cookie", &member.cookie)
                .header("Content-Type", "application/json")
                .header("Idempotency-Key", Uuid::new_v4().to_string())
                .body(Body::from("{"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/problem+json"
    );
    assert_eq!(body_json(response).await["code"], "invalid_request");
}
