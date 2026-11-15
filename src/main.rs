use equipment_reservation_api::{AppState, migrate, router};
use sqlx::postgres::PgPoolOptions;
use std::{env, net::SocketAddr};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("equipment_reservation_api=info,tower_http=info")
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(15)
        .connect(&database_url)
        .await?;
    migrate(&pool).await?;

    let app_env = env::var("APP_ENV").unwrap_or_else(|_| "production".to_owned());
    let demo_auth_enabled = app_env == "development"
        && env::var("DEMO_AUTH_ENABLED")
            .map(|value| value == "true")
            .unwrap_or(true);
    let state = AppState {
        pool,
        demo_auth_enabled,
    };
    let app = router(state).layer(TraceLayer::new_for_http());

    let port = env::var("PORT")
        .unwrap_or_else(|_| "8080".to_owned())
        .parse::<u16>()?;
    let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port))).await?;
    tracing::info!(port, demo_auth_enabled, "API server listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
