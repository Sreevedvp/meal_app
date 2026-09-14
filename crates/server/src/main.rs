use std::{net::SocketAddr, path::Path, time::Duration};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "meal_server=info".into()),
        )
        .init();
    let url =
        std::env::var("DATABASE_URL").map_err(|_| "Set DATABASE_URL; see docs/DEPLOYMENT.md.")?;
    if std::env::args().nth(1).as_deref() == Some("migrate") {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&url)
            .await?;
        sqlx::migrate!().run(&pool).await?;
        pool.close().await;
        tracing::info!("Database migrations completed");
        return Ok(());
    }
    let config = meal_server::security::Config::from_env()?;
    let bind: SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8081".into())
        .parse()
        .map_err(|_| "BIND_ADDR must be an IP address and port.")?;
    config.validate_bind(bind)?;
    let public = std::env::var("WEB_DIST")
        .unwrap_or_else(|_| "target/dx/meal-app/release/web/public".into());
    if !Path::new(&public).join("index.html").is_file() {
        return Err("WEB_DIST does not contain index.html. Build the web client first.".into());
    }
    let production = config.production;
    let state = meal_server::connect_configured(&url, config, !production).await?;
    // Fail on an outdated schema before accepting traffic; production migrations are explicit.
    sqlx::query("SELECT owner_id,revision FROM fitness_plans LIMIT 0")
        .execute(&state.db)
        .await?;
    let pool = state.db.clone();
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind,production,"Deccan Table ready");
    axum::serve(listener, meal_server::app(state, &public))
        .with_graceful_shutdown(shutdown())
        .await?;
    pool.close().await;
    Ok(())
}
async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {_ = ctrl_c=>{},_ = terminate=>{}}
    tracing::info!("Graceful shutdown started");
}
