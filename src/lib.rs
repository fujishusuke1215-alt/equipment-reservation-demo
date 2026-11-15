mod api;
mod auth;
mod equipment;
mod errors;
mod models;
mod reservations;
mod state;

pub use api::router;
pub use state::AppState;

use sqlx::PgPool;

pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}
