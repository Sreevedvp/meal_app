//! Run against the dedicated local database: cargo test -p meal-server --test postgres_flow -- --ignored
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use meal_domain::*;
use tower::ServiceExt;
async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .header(
                    "if-match",
                    format!(
                        "\"{}\"",
                        if method == "DELETE" {
                            body.as_u64().unwrap_or(1)
                        } else {
                            1
                        }
                    ),
                )
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        if body.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&body).unwrap()
        },
    )
}
#[tokio::test]
#[ignore = "requires DATABASE_URL pointing to a disposable local PostgreSQL database"]
async fn saved_plan_swap_and_authoritative_validation() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let state = meal_server::connect(&url).await.unwrap();
    let app = meal_server::app(state.clone(), "missing");
    let request = PlanRequest {
        allergens: vec!["milk".into(), "peanut".into()],
        budget_paise: 300_000,
        ..Default::default()
    };
    let (status, body) = call(
        &app,
        "POST",
        "/api/plans",
        serde_json::to_value(&request).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let plan: Plan = serde_json::from_value(body).unwrap();
    assert_eq!(plan.slots.len(), 21);
    assert!(plan.within_estimate);
    assert!(
        !plan
            .groceries
            .iter()
            .any(|g| g.ingredient_id == "milk" || g.ingredient_id == "peanut")
    );
    let uri = format!("/api/plans/{}", plan.id);
    let (status, body) = call(&app, "GET", &uri, serde_json::Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_value::<Plan>(body).unwrap(), plan);
    let swap_uri = format!("{uri}/swap");
    // Even extra client-supplied preferences cannot replace the stored exclusions.
    let (status,body)=call(&app,"POST",&swap_uri,serde_json::json!({"slot_index":0,"recipe_id":"peanut-poha","allergens":[],"budget_paise":9999999})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["message"].as_str().unwrap().contains("peanut"));
    let (status, _) = call(
        &app,
        "POST",
        &swap_uri,
        serde_json::json!({"slot_index":1,"recipe_id":"mix-rice"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let next_id = if plan.slots[0].recipe_id == "moong-chilla" {
        "vegetable-upma"
    } else {
        "moong-chilla"
    };
    let (status, body) = call(
        &app,
        "POST",
        &swap_uri,
        serde_json::json!({"slot_index":0,"recipe_id":next_id}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let next: Plan = serde_json::from_value(body).unwrap();
    assert_ne!(next.id, plan.id);
    assert_eq!(next.slots[1..], plan.slots[1..]);
    assert_eq!(
        next.groceries,
        groceries(&state.catalogue, &request, &next.slots).unwrap()
    );
    let (_, original) = call(&app, "GET", &uri, serde_json::Value::Null).await;
    assert_eq!(serde_json::from_value::<Plan>(original).unwrap(), plan);
    let low = PlanRequest {
        budget_paise: 1,
        ..request.clone()
    };
    let (status, body) = call(
        &app,
        "POST",
        "/api/plans",
        serde_json::to_value(low).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["within_estimate"], false);
    let low_id = body["id"].as_str().unwrap();
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/plans/{low_id}/swap"),
        serde_json::json!({"slot_index":0,"recipe_id":"moong-chilla"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (_, health) = call(&app, "GET", "/api/health", serde_json::Value::Null).await;
    assert_eq!(health["status"], "ok");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL pointing to the local development database"]
async fn indexed_food_catalogue_and_paginated_search() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let state = meal_server::connect(&url).await.unwrap();
    let expected = demo_catalogue();
    assert_eq!(state.catalogue.recipes.len(), expected.recipes.len());
    let count: (i64,) =
        sqlx::query_as("SELECT count(*) FROM food_recipes WHERE catalogue_version=$1")
            .bind(CATALOGUE_VERSION)
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(count.0 as usize, expected.recipes.len());
    // Startup is idempotent, including normalized ingredient relations.
    meal_server::connect(&url).await.unwrap();
    let links: (i64,) =
        sqlx::query_as("SELECT count(*) FROM food_recipe_ingredients WHERE catalogue_version=$1")
            .bind(CATALOGUE_VERSION)
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(
        links.0 as usize,
        expected
            .recipes
            .iter()
            .map(|r| r.ingredients.len())
            .sum::<usize>()
    );
    assert_eq!(
        estimate_budget(&state.catalogue, &FitnessProfile::default()).unwrap(),
        estimate_budget(&expected, &FitnessProfile::default()).unwrap()
    );
    let app = meal_server::app(state, "missing");
    let (status, body) = call(&app, "GET", "/api/foods?limit=10", serde_json::Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    let first: FoodPage = serde_json::from_value(body).unwrap();
    assert_eq!(first.items.len(), 10);
    assert_eq!(first.total as usize, expected.recipes.len());
    let (_, body) = call(
        &app,
        "GET",
        "/api/foods?limit=10&offset=10",
        serde_json::Value::Null,
    )
    .await;
    let next: FoodPage = serde_json::from_value(body).unwrap();
    assert!(
        next.items
            .iter()
            .all(|r| !first.items.iter().any(|f| f.id == r.id))
    );
    let (_, body) = call(
        &app,
        "GET",
        "/api/foods?q=paneer&meal=breakfast&diet=vegetarian",
        serde_json::Value::Null,
    )
    .await;
    let matching: FoodPage = serde_json::from_value(body).unwrap();
    assert!(matching.total > 0);
    assert!(matching.items.iter().all(|r| r.vegetarian
        && r.meals.contains(&Meal::Breakfast)
        && r.name.to_lowercase().contains("paneer")));
    let (_, body) = call(
        &app,
        "GET",
        "/api/foods?offset=9999",
        serde_json::Value::Null,
    )
    .await;
    let empty: FoodPage = serde_json::from_value(body).unwrap();
    assert!(empty.items.is_empty());
    assert_eq!(empty.total, first.total);
    for uri in [
        "/api/foods?limit=1001",
        "/api/foods?offset=-1",
        "/api/foods?meal=snack",
        "/api/foods?diet=unknown",
    ] {
        assert_eq!(
            call(&app, "GET", uri, serde_json::Value::Null).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}

#[tokio::test]
#[ignore = "requires DATABASE_URL pointing to the local development database"]
async fn fitness_calendar_save_reload_and_authoritative_validation() {
    let url = std::env::var("DATABASE_URL").unwrap();
    let state = meal_server::connect(&url).await.unwrap();
    let request = PlannerRequest {
        profile: FitnessProfile::default(),
        start_date: "2026-12-28".into(),
        days: 7,
    };
    let plan = generate_fitness_plan(&state.catalogue, &request).unwrap();
    let app = meal_server::app(state.clone(), "missing");
    let mut input = PlannerSaveRequest {
        revision: Some(1),
        request: request.clone(),
        meals: plan.meals.clone(),
    };
    input.meals[0].nutrition.protein_g = 9999.0;
    let (status, body) = call(
        &app,
        "POST",
        "/api/fitness-plans",
        serde_json::to_value(&input).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let saved: StoredFitnessPlan = serde_json::from_value(body).unwrap();
    assert!(!saved.plan.id.is_empty());
    assert_ne!(saved.plan.meals[0].nutrition.protein_g, 9999.0);
    assert_eq!(saved.plan.high_paise, plan.high_paise);
    let uri = format!("/api/fitness-plans/{}", saved.plan.id);
    let (status, body) = call(&app, "GET", &uri, serde_json::Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    let loaded: StoredFitnessPlan = serde_json::from_value(body).unwrap();
    assert_eq!(loaded.plan.id, saved.plan.id);
    assert_eq!(loaded.plan.meals, saved.plan.meals);
    assert_eq!(loaded.catalogue.version, saved.plan.catalogue_version);
    input.meals[1] = input.meals[0].clone();
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/fitness-plans",
            serde_json::to_value(&input).unwrap()
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    input.meals = plan.meals.clone();
    input.request.profile.monthly_budget_paise = 1;
    input.request.profile.diet_first = false;
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/fitness-plans",
            serde_json::to_value(&input).unwrap()
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    input.request = request.clone();
    input
        .request
        .profile
        .excluded_ingredients
        .push("salt".into());
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/fitness-plans",
            serde_json::to_value(&input).unwrap()
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (index, choice) = (0..plan.meals.len())
        .find_map(|i| {
            fitness_swap_options(&state.catalogue, &plan, i)
                .first()
                .cloned()
                .map(|o| (i, o))
        })
        .unwrap();
    let swapped = swap_fitness_meal(
        &state.catalogue,
        &plan,
        index,
        &choice.recipe_id,
        choice.portion_percent,
    )
    .unwrap();
    let (_, body) = call(
        &app,
        "POST",
        "/api/fitness-plans",
        serde_json::to_value(PlannerSaveRequest {
            revision: Some(1),
            request,
            meals: swapped.meals,
        })
        .unwrap(),
    )
    .await;
    let second: StoredFitnessPlan = serde_json::from_value(body).unwrap();
    assert_ne!(second.plan.id, saved.plan.id);
    let (_, body) = call(&app, "GET", &uri, serde_json::Value::Null).await;
    assert_eq!(
        serde_json::from_value::<StoredFitnessPlan>(body)
            .unwrap()
            .plan
            .meals,
        saved.plan.meals
    );
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/fitness-plans/not-an-id",
            serde_json::Value::Null
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL pointing to the local development database"]
async fn fitness_calendar_edit_copy_and_delete() {
    let state = meal_server::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();
    let request = PlannerRequest {
        profile: FitnessProfile::default(),
        start_date: "2026-09-01".into(),
        days: 7,
    };
    let plan = generate_fitness_plan(&state.catalogue, &request).unwrap();
    let app = meal_server::app(state.clone(), "missing");
    let mut input = PlannerSaveRequest {
        revision: Some(1),
        request,
        meals: plan.meals,
    };
    let (_, body) = call(
        &app,
        "POST",
        "/api/fitness-plans",
        serde_json::to_value(&input).unwrap(),
    )
    .await;
    let original: StoredFitnessPlan = serde_json::from_value(body).unwrap();
    let uri = format!("/api/fitness-plans/{}", original.plan.id);
    input.request.start_date = "2026-10-01".into();
    input.meals[0].nutrition.protein_g = 9999.0;
    // Updates use the historical catalogue, even if a running API has different fixtures.
    let mut alternate = state.clone();
    std::sync::Arc::make_mut(&mut alternate.catalogue)
        .recipes
        .clear();
    let alternate_app = meal_server::app(alternate, "missing");
    let (status, body) = call(
        &alternate_app,
        "PUT",
        &uri,
        serde_json::to_value(&input).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let edited: StoredFitnessPlan = serde_json::from_value(body).unwrap();
    assert_eq!(edited.plan.id, original.plan.id);
    assert_eq!(edited.plan.request.start_date, "2026-10-01");
    assert_eq!(edited.plan.meals, original.plan.meals);
    input.revision = Some(edited.revision);
    input
        .request
        .profile
        .excluded_ingredients
        .push("salt".into());
    assert_eq!(
        call(&app, "PUT", &uri, serde_json::to_value(&input).unwrap())
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (_, body) = call(&app, "GET", &uri, serde_json::Value::Null).await;
    assert_eq!(
        serde_json::from_value::<StoredFitnessPlan>(body).unwrap(),
        edited
    );
    input.request.profile.excluded_ingredients.clear();
    input.request.profile.monthly_budget_paise = 1;
    input.request.profile.diet_first = false;
    assert_eq!(
        call(&app, "PUT", &uri, serde_json::to_value(&input).unwrap())
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    input.request.profile = FitnessProfile::default();
    input.request.start_date = "2026-11-01".into();
    let (_, body) = call(
        &app,
        "POST",
        "/api/fitness-plans",
        serde_json::to_value(&input).unwrap(),
    )
    .await;
    let copy: StoredFitnessPlan = serde_json::from_value(body).unwrap();
    assert_ne!(copy.plan.id, edited.plan.id);
    assert_eq!(copy.plan.meals, edited.plan.meals);
    assert_eq!(
        call(&app, "DELETE", &uri, serde_json::json!(edited.revision))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        call(&app, "GET", &uri, serde_json::Value::Null).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&app, "DELETE", &uri, serde_json::json!(edited.revision))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        call(&app, "PUT", &uri, serde_json::to_value(&input).unwrap())
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            &app,
            "DELETE",
            "/api/fitness-plans/invalid",
            serde_json::Value::Null
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let copy_uri = format!("/api/fitness-plans/{}", copy.plan.id);
    assert_eq!(
        call(&app, "GET", &copy_uri, serde_json::Value::Null)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "DELETE", &copy_uri, serde_json::Value::Null)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
}

async fn authenticated(
    app: &Router,
    method: &str,
    uri: &str,
    body: serde_json::Value,
    user: &str,
    origin: &str,
    revision: Option<u64>,
) -> (StatusCode, serde_json::Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header(
            "x-proxy-secret",
            "test-only-proxy-secret-abcdefghijklmnopqrstuvwxyz",
        )
        .header("x-forwarded-user", user)
        .header("origin", origin);
    if let Some(revision) = revision {
        request = request.header("if-match", format!("\"{revision}\""));
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert!(response.headers().contains_key("x-request-id"));
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

#[tokio::test]
#[ignore = "requires DATABASE_URL pointing to a disposable PostgreSQL database"]
async fn production_isolation_csrf_conflicts_and_quota() {
    use serde_json::{Value, json};
    let config = meal_server::security::Config {
        production: true,
        proxy_secret: Some("test-only-proxy-secret-abcdefghijklmnopqrstuvwxyz".into()),
        identity_namespace: format!("https://issuer.example/{}", uuid::Uuid::new_v4()),
        origins: vec!["https://meal.example".into()],
        max_saved_plans: 1,
        ..Default::default()
    };
    let state =
        meal_server::connect_configured(&std::env::var("DATABASE_URL").unwrap(), config, true)
            .await
            .unwrap();
    let request = PlannerRequest {
        profile: FitnessProfile::default(),
        start_date: "2026-09-01".into(),
        days: 7,
    };
    let plan = generate_fitness_plan(&state.catalogue, &request).unwrap();
    let input = json!(PlannerSaveRequest {
        revision: Some(1),
        request,
        meals: plan.meals
    });
    let app = meal_server::app(state, "missing");
    let base = "/api/fitness-plans";
    let origin = "https://meal.example";
    assert_eq!(
        call(&app, "GET", base, Value::Null).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        authenticated(
            &app,
            "POST",
            base,
            input.clone(),
            "alice",
            "https://evil.example",
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        authenticated(&app, "POST", "/api/plans", json!({}), "alice", origin, None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (status, created) =
        authenticated(&app, "POST", base, input.clone(), "alice", origin, None).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let uri = format!("{base}/{}", created["plan"]["id"].as_str().unwrap());
    assert_eq!(
        authenticated(&app, "POST", base, input.clone(), "alice", origin, None)
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        authenticated(&app, "GET", &uri, Value::Null, "bob", origin, None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        authenticated(&app, "PUT", &uri, input.clone(), "bob", origin, None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        authenticated(&app, "DELETE", &uri, Value::Null, "bob", origin, Some(1))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let (_, bob) = authenticated(&app, "GET", base, Value::Null, "bob", origin, None).await;
    assert_eq!(bob["items"].as_array().unwrap().len(), 0);
    let (_, alice) = authenticated(&app, "GET", base, Value::Null, "alice", origin, None).await;
    assert_eq!(alice["items"].as_array().unwrap().len(), 1);
    assert_eq!(alice["items"][0]["id"], created["plan"]["id"]);
    let mut unversioned = input.clone();
    unversioned["revision"] = Value::Null;
    assert_eq!(
        authenticated(&app, "PUT", &uri, unversioned, "alice", origin, None)
            .await
            .0,
        StatusCode::PRECONDITION_REQUIRED
    );
    let (status, updated) =
        authenticated(&app, "PUT", &uri, input.clone(), "alice", origin, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["revision"], 2);
    assert_eq!(
        authenticated(&app, "PUT", &uri, input, "alice", origin, None)
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        authenticated(&app, "DELETE", &uri, Value::Null, "alice", origin, None)
            .await
            .0,
        StatusCode::PRECONDITION_REQUIRED
    );
    assert_eq!(
        authenticated(&app, "DELETE", &uri, Value::Null, "alice", origin, Some(1))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        authenticated(&app, "DELETE", &uri, Value::Null, "alice", origin, Some(2))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        authenticated(&app, "DELETE", &uri, Value::Null, "alice", origin, Some(2))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
}
