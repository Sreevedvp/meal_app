use crate::*;
use axum::extract::Query;
use serde::Deserialize;

/// Seed individual records atomically; an existing catalogue version is immutable.
pub async fn seed(db: &PgPool, c: &Catalogue) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;
    for i in &c.ingredients {
        sqlx::query("INSERT INTO food_ingredients(catalogue_version,id,name,data) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(&c.version).bind(&i.id).bind(&i.name).bind(DbJson(i)).execute(&mut *tx).await?;
    }
    for r in &c.recipes {
        let meals: Vec<_> = r.meals.iter().map(|m| m.label().to_lowercase()).collect();
        sqlx::query("INSERT INTO food_recipes(catalogue_version,id,name,cuisine,category,vegetarian,meal_types,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING")
            .bind(&c.version).bind(&r.id).bind(&r.name).bind(&r.cuisine).bind(&r.category).bind(r.vegetarian).bind(meals).bind(DbJson(r)).execute(&mut *tx).await?;
        for a in &r.ingredients {
            sqlx::query("INSERT INTO food_recipe_ingredients(catalogue_version,recipe_id,ingredient_id,quantity) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
                .bind(&c.version).bind(&r.id).bind(&a.ingredient).bind(a.quantity as i64).execute(&mut *tx).await?;
        }
    }
    tx.commit().await
}
pub async fn load_catalogue(db: &PgPool, version: &str) -> Result<Catalogue, sqlx::Error> {
    let ingredients: Vec<(DbJson<Ingredient>,)> =
        sqlx::query_as("SELECT data FROM food_ingredients WHERE catalogue_version=$1 ORDER BY id")
            .bind(version)
            .fetch_all(db)
            .await?;
    let recipes: Vec<(DbJson<Recipe>,)> =
        sqlx::query_as("SELECT data FROM food_recipes WHERE catalogue_version=$1 ORDER BY id")
            .bind(version)
            .fetch_all(db)
            .await?;
    Ok(Catalogue {
        version: version.into(),
        ingredients: ingredients.into_iter().map(|(DbJson(i),)| i).collect(),
        recipes: recipes.into_iter().map(|(DbJson(r),)| r).collect(),
    })
}
#[derive(Default, Deserialize)]
pub struct FoodQuery {
    #[serde(default)]
    q: String,
    meal: Option<String>,
    diet: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}
pub async fn search(
    State(s): State<AppState>,
    Query(q): Query<FoodQuery>,
) -> Result<Json<FoodPage>, Error> {
    if q.q.len() > 120
        || q.meal
            .as_ref()
            .is_some_and(|m| !["breakfast", "lunch", "dinner"].contains(&m.as_str()))
        || q.diet
            .as_ref()
            .is_some_and(|d| !["vegetarian", "all"].contains(&d.as_str()))
    {
        return Err(invalid(
            "Use a search of at most 120 bytes, a valid meal and vegetarian/all diet.".into(),
        ));
    }
    let limit = q.limit.unwrap_or(12);
    let offset = q.offset.unwrap_or(0);
    if !(1..=100).contains(&limit) || !(0..=1_000_000).contains(&offset) {
        return Err(invalid(
            "Limit must be 1–100 and offset 0–1,000,000.".into(),
        ));
    }
    let vegetarian = q.diet.as_deref() == Some("vegetarian");
    // Window count and results use a single statement/snapshot; an empty page still
    // carries the total via the LEFT JOIN. All search inputs are bound parameters.
    let rows: Vec<(i64, Option<DbJson<Recipe>>)> = sqlx::query_as(r#"
        WITH filtered AS NOT MATERIALIZED (
            SELECT id,name,data FROM food_recipes
            WHERE catalogue_version=$1
              AND ($2='' OR search_document @@ plainto_tsquery('simple',$2))
              AND ($3::text IS NULL OR meal_types @> ARRAY[$3]::text[])
              AND (NOT $4 OR vegetarian)
        ), page AS (SELECT id,name,data FROM filtered ORDER BY name,id LIMIT $5 OFFSET $6)
        SELECT total.count,page.data FROM (SELECT count(*) FROM filtered) total LEFT JOIN page ON true ORDER BY page.name,page.id
    "#).bind(&s.catalogue.version).bind(q.q.trim()).bind(q.meal).bind(vegetarian).bind(limit).bind(offset).fetch_all(&s.db).await?;
    let total = rows.first().map(|r| r.0).unwrap_or(0) as u64;
    Ok(Json(FoodPage {
        total,
        items: rows
            .into_iter()
            .filter_map(|(_, r)| r.map(|DbJson(r)| r))
            .collect(),
        limit: limit as usize,
        offset: offset as usize,
    }))
}
