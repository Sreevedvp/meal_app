mod foods;
mod planner;
pub mod security;
use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    http::{HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use meal_domain::*;
use sqlx::{PgPool, postgres::PgPoolOptions, types::Json as DbJson};
use std::sync::Arc;
use tower_http::{cors::CorsLayer, services::ServeDir};
use uuid::Uuid;
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub catalogue: Arc<Catalogue>,
    pub security: Arc<security::Runtime>,
}
#[derive(Debug)]
pub struct Error(StatusCode, String);
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, Json(ApiError { message: self.1 })).into_response()
    }
}
impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!(error=%e,"Database request failed");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not save or load the plan. Please retry.".into(),
        )
    }
}
fn invalid(e: String) -> Error {
    Error(StatusCode::UNPROCESSABLE_ENTITY, e)
}
pub async fn connect(url: &str) -> Result<AppState, Box<dyn std::error::Error>> {
    connect_configured(url, security::Config::default(), true).await
}
pub async fn connect_configured(
    url: &str,
    config: security::Config,
    migrate: bool,
) -> Result<AppState, Box<dyn std::error::Error>> {
    let db = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .after_connect(|conn, _| {
            Box::pin(async move {
                sqlx::query("SET statement_timeout = '10s'")
                    .execute(&mut *conn)
                    .await?;
                sqlx::query("SET lock_timeout = '3s'").execute(conn).await?;
                Ok(())
            })
        })
        .connect(url)
        .await?;
    if migrate {
        sqlx::migrate!().run(&db).await?;
    }
    let catalogue = demo_catalogue();
    sqlx::query(
        "INSERT INTO catalogues(version,data) VALUES($1,$2) ON CONFLICT(version) DO NOTHING",
    )
    .bind(&catalogue.version)
    .bind(DbJson(&catalogue))
    .execute(&db)
    .await?;
    let (DbJson(stored),): (DbJson<Catalogue>,) =
        sqlx::query_as("SELECT data FROM catalogues WHERE version=$1")
            .bind(&catalogue.version)
            .fetch_one(&db)
            .await?;
    foods::seed(&db, &stored).await?;
    let stored = foods::load_catalogue(&db, &catalogue.version).await?;
    Ok(AppState {
        db,
        catalogue: Arc::new(stored),
        security: Arc::new(security::Runtime::new(config)),
    })
}
pub fn app(state: AppState, static_dir: &str) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(
            state
                .security
                .config
                .origins
                .iter()
                .map(|s| HeaderValue::from_str(s).expect("validated origin"))
                .collect::<Vec<_>>(),
        )
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::IF_MATCH,
        ]);
    Router::new()
        .route("/api/health", get(health))
        .route("/api/ready", get(health))
        .route(
            "/api/live",
            get(|| async { Json(serde_json::json!({"status":"ok"})) }),
        )
        .route("/api/session", get(session))
        .route("/api/catalogue", get(catalogue))
        .route("/api/foods", get(foods::search))
        .route("/api/fitness-plans", post(planner::save).get(planner::list))
        .route(
            "/api/fitness-plans/{id}",
            get(planner::load)
                .put(planner::update)
                .delete(planner::delete),
        )
        .route("/api/plans", post(create))
        .route("/api/plans/{id}", get(load))
        .route("/api/plans/{id}/swap", post(replace))
        .fallback_service(ServeDir::new(static_dir))
        .layer(axum::extract::DefaultBodyLimit::max(32768))
        .layer(cors)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            security::guard,
        ))
        .with_state(state)
}
async fn health(State(s): State<AppState>) -> Result<Json<serde_json::Value>, Error> {
    sqlx::query("SELECT 1").execute(&s.db).await?;
    Ok(Json(serde_json::json!({"status":"ok"})))
}
async fn session(
    State(s): State<AppState>,
    Extension(owner): Extension<security::Principal>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({"storage_scope":owner.0,"production":s.security.config.production}))
}
async fn catalogue(State(s): State<AppState>) -> Json<Catalogue> {
    Json((*s.catalogue).clone())
}
async fn enforce_quota(
    s: &AppState,
    owner: &security::Principal,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<(), Error> {
    // Serialize each owner's creates so concurrent requests cannot exceed the quota.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(&owner.0)
        .execute(&mut **tx)
        .await?;
    let count:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM plans WHERE owner_id=$1)+(SELECT count(*) FROM fitness_plans WHERE owner_id=$1)").bind(&owner.0).fetch_one(&mut **tx).await?;
    if count >= s.security.config.max_saved_plans {
        return Err(Error(
            StatusCode::CONFLICT,
            "Saved-plan limit reached. Delete an older online plan before saving another.".into(),
        ));
    }
    Ok(())
}
async fn persist(s: &AppState, owner: &security::Principal, mut plan: Plan) -> Result<Plan, Error> {
    plan.id = Uuid::new_v4().to_string();
    let mut tx = s.db.begin().await?;
    enforce_quota(s, owner, &mut tx).await?;
    sqlx::query(
        "INSERT INTO plans(id,catalogue_version,request,result,owner_id) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(Uuid::parse_str(&plan.id).unwrap())
    .bind(&plan.catalogue_version)
    .bind(DbJson(&plan.request))
    .bind(DbJson(&plan))
    .bind(&owner.0)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(plan)
}
async fn create(
    State(s): State<AppState>,
    Extension(owner): Extension<security::Principal>,
    Json(request): Json<PlanRequest>,
) -> Result<Json<Plan>, Error> {
    let c = s.catalogue.clone();
    let result = tokio::task::spawn_blocking(move || generate(&c, &request))
        .await
        .map_err(|_| {
            Error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Planning failed; please retry.".into(),
            )
        })?
        .map_err(invalid)?;
    Ok(Json(persist(&s, &owner, result).await?))
}
async fn find(s: &AppState, owner: &security::Principal, id: &str) -> Result<Plan, Error> {
    let id = Uuid::parse_str(id).map_err(|_| invalid("Invalid plan ID".into()))?;
    let row: Option<(DbJson<Plan>,)> =
        sqlx::query_as("SELECT result FROM plans WHERE id=$1 AND owner_id=$2")
            .bind(id)
            .bind(&owner.0)
            .fetch_optional(&s.db)
            .await?;
    row.map(|(DbJson(p),)| p)
        .ok_or(Error(StatusCode::NOT_FOUND, "Plan not found".into()))
}
async fn load(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Extension(owner): Extension<security::Principal>,
) -> Result<Json<Plan>, Error> {
    Ok(Json(find(&s, &owner, &id).await?))
}
async fn replace(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Extension(owner): Extension<security::Principal>,
    Json(request): Json<SwapRequest>,
) -> Result<Json<Plan>, Error> {
    // Load stored preferences and slots. Client cannot supply a revised budget or allergy list here.
    let original = find(&s, &owner, &id).await?;
    let plan = swap(
        &s.catalogue,
        &original,
        request.slot_index,
        &request.recipe_id,
    )
    .map_err(invalid)?;
    Ok(Json(persist(&s, &owner, plan).await?))
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    #[tokio::test]
    async fn http_validation_happens_before_database_write() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@localhost/unused")
            .unwrap();
        let state = AppState {
            db: pool,
            catalogue: Arc::new(demo_catalogue()),
            security: Arc::new(security::Runtime::new(security::Config::default())),
        };
        let r = PlanRequest {
            allergens: vec!["unmapped".into()],
            ..Default::default()
        };
        let response = app(state, "missing")
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/plans")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&r).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
