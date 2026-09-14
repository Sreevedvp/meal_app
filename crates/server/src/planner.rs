use crate::*;
use axum::http::HeaderMap;
use security::Principal;

pub async fn save(
    State(s): State<AppState>,
    Extension(owner): Extension<Principal>,
    Json(input): Json<PlannerSaveRequest>,
) -> Result<Json<StoredFitnessPlan>, Error> {
    let c = s.catalogue.clone();
    let mut plan =
        tokio::task::spawn_blocking(move || finish_fitness_plan(&c, &input.request, input.meals))
            .await
            .map_err(|_| {
                Error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Could not validate this plan.".into(),
                )
            })?
            .map_err(invalid)?;
    plan.id = Uuid::new_v4().to_string();
    let mut tx = s.db.begin().await?;
    enforce_quota(&s, &owner, &mut tx).await?;
    sqlx::query("INSERT INTO fitness_plans(id,catalogue_version,request,result,owner_id) VALUES($1,$2,$3,$4,$5)")
        .bind(Uuid::parse_str(&plan.id).expect("generated UUID")).bind(&plan.catalogue_version)
        .bind(DbJson(&plan.request)).bind(DbJson(&plan)).bind(&owner.0).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(StoredFitnessPlan {
        revision: 1,
        plan,
        catalogue: (*s.catalogue).clone(),
    }))
}

pub async fn load(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Extension(owner): Extension<Principal>,
) -> Result<Json<StoredFitnessPlan>, Error> {
    let id = Uuid::parse_str(&id).map_err(|_| invalid("Enter a valid saved plan ID.".into()))?;
    let row:Option<(DbJson<FitnessPlan>,DbJson<Catalogue>,i64)>=sqlx::query_as("SELECT p.result,c.data,p.revision FROM fitness_plans p JOIN catalogues c ON c.version=p.catalogue_version WHERE p.id=$1 AND p.owner_id=$2")
        .bind(id).bind(&owner.0).fetch_optional(&s.db).await?;
    row.map(|(DbJson(plan), DbJson(catalogue), revision)| {
        Json(StoredFitnessPlan {
            revision: revision as u64,
            plan,
            catalogue,
        })
    })
    .ok_or(Error(StatusCode::NOT_FOUND, "Saved plan not found.".into()))
}

pub async fn update(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Extension(owner): Extension<Principal>,
    Json(input): Json<PlannerSaveRequest>,
) -> Result<Json<StoredFitnessPlan>, Error> {
    let uuid = Uuid::parse_str(&id).map_err(|_| invalid("Enter a valid saved plan ID.".into()))?;
    let Json(original) = load(State(s.clone()), Path(id), Extension(owner.clone())).await?;
    let expected = input
        .revision
        .filter(|v| *v > 0 && *v < i64::MAX as u64)
        .ok_or(Error(
            StatusCode::PRECONDITION_REQUIRED,
            "Reload the latest saved plan before editing.".into(),
        ))?;
    if expected != original.revision {
        return Err(conflict());
    }
    let c = original.catalogue.clone();
    let mut plan =
        tokio::task::spawn_blocking(move || finish_fitness_plan(&c, &input.request, input.meals))
            .await
            .map_err(|_| {
                Error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Could not validate this plan.".into(),
                )
            })?
            .map_err(invalid)?;
    plan.id = uuid.to_string();
    let changed=sqlx::query("UPDATE fitness_plans SET request=$2,result=$3,revision=revision+1,updated_at=now() WHERE id=$1 AND owner_id=$4 AND revision=$5")
        .bind(uuid).bind(DbJson(&plan.request)).bind(DbJson(&plan)).bind(&owner.0).bind(expected as i64).execute(&s.db).await?;
    if changed.rows_affected() == 0 {
        return Err(conflict());
    }
    Ok(Json(StoredFitnessPlan {
        revision: expected + 1,
        plan,
        catalogue: original.catalogue,
    }))
}

pub async fn delete(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Extension(owner): Extension<Principal>,
    headers: HeaderMap,
) -> Result<StatusCode, Error> {
    let id = Uuid::parse_str(&id).map_err(|_| invalid("Enter a valid saved plan ID.".into()))?;
    let revision = headers
        .get("if-match")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.trim_matches('"').parse::<i64>().ok())
        .filter(|v| *v > 0)
        .ok_or(Error(
            StatusCode::PRECONDITION_REQUIRED,
            "Reload the latest saved plan before deleting.".into(),
        ))?;
    let result =
        sqlx::query("DELETE FROM fitness_plans WHERE id=$1 AND owner_id=$2 AND revision=$3")
            .bind(id)
            .bind(&owner.0)
            .bind(revision)
            .execute(&s.db)
            .await?;
    if result.rows_affected() == 0 {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM fitness_plans WHERE id=$1 AND owner_id=$2)",
        )
        .bind(id)
        .bind(&owner.0)
        .fetch_one(&s.db)
        .await?;
        if exists {
            return Err(conflict());
        }
    }
    Ok(StatusCode::NO_CONTENT)
}
fn conflict() -> Error {
    Error(StatusCode::CONFLICT,"This plan changed elsewhere. Open its latest saved version before editing or deleting. Your draft is unchanged.".into())
}

#[derive(serde::Deserialize)]
pub struct Page {
    #[serde(default)]
    offset: i64,
}
pub async fn list(
    State(s): State<AppState>,
    Extension(owner): Extension<Principal>,
    axum::extract::Query(page): axum::extract::Query<Page>,
) -> Result<Json<serde_json::Value>, Error> {
    if !(0..=1000).contains(&page.offset) {
        return Err(invalid("Invalid page offset.".into()));
    }
    let rows:Vec<(DbJson<serde_json::Value>,)>=sqlx::query_as("SELECT jsonb_build_object('id',id,'start_date',request->>'start_date','days',request->'days','high_paise',result->'high_paise','revision',revision) FROM fitness_plans WHERE owner_id=$1 ORDER BY updated_at DESC,id LIMIT 21 OFFSET $2")
        .bind(&owner.0).bind(page.offset).fetch_all(&s.db).await?;
    Ok(Json(
        serde_json::json!({"has_more":rows.len()>20,"offset":page.offset,"items":rows.into_iter().take(20).map(|(DbJson(v),)|v).collect::<Vec<_>>()}),
    ))
}
