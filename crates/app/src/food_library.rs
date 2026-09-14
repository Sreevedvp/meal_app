use super::*;

#[component]
pub fn FoodLibrary(
    settings: Signal<Settings>,
    catalogue: Signal<Catalogue>,
    connected: Signal<bool>,
) -> Element {
    let mut draft = use_signal(String::new);
    let mut query = use_signal(String::new);
    let mut meal = use_signal(String::new);
    let mut offset = use_signal(|| 0_usize);
    let remote = use_resource(move || async move {
        let s = settings();
        let query = query();
        let meal = meal();
        let offset = offset();
        if !connected() || s.endpoint.is_empty() {
            return Ok(None);
        }
        let mut params = vec![
            ("q", query),
            ("offset", offset.to_string()),
            ("limit", "12".into()),
            (
                "diet",
                if s.profile.diet == Diet::Vegetarian {
                    "vegetarian"
                } else {
                    "all"
                }
                .into(),
            ),
        ];
        if !meal.is_empty() {
            params.push(("meal", meal));
        }
        let response = reqwest::Client::new()
            .get(format!("{}/api/foods", s.endpoint.trim_end_matches('/')))
            .query(&params)
            .timeout(std::time::Duration::from_secs(8))
            .send()
            .await
            .map_err(|_| {
                "Food search is unavailable. Showing bundled foods instead.".to_string()
            })?;
        if !response.status().is_success() {
            return Err("Food search is unavailable. Showing bundled foods instead.".to_string());
        }
        response.json::<FoodPage>().await.map(Some).map_err(|_| {
            "Food search returned unreadable data. Showing bundled foods instead.".to_string()
        })
    });
    let ready = matches!(
        (remote.state())(),
        UseResourceState::Ready | UseResourceState::Stopped
    );
    let c = catalogue();
    let p = settings().profile;
    let terms: Vec<String> = query().split_whitespace().map(str::to_lowercase).collect();
    let mut local: Vec<_> = c
        .recipes
        .iter()
        .filter(|r| {
            let haystack = format!("{} {} {}", r.name, r.cuisine, r.category).to_lowercase();
            terms.iter().all(|term| haystack.contains(term))
                && (meal().is_empty() || r.meals.iter().any(|m| m.label().to_lowercase() == meal()))
                && (p.diet != Diet::Vegetarian || r.vegetarian)
        })
        .cloned()
        .collect();
    local.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
    let mut page = FoodPage {
        total: local.len() as u64,
        items: local.into_iter().skip(offset()).take(12).collect(),
        limit: 12,
        offset: offset(),
    };
    let mut source = "Bundled catalogue";
    let mut warning = None;
    if ready {
        match remote.read().as_ref() {
            Some(Ok(Some(result))) => {
                page = result.clone();
                source = "PostgreSQL catalogue";
            }
            Some(Err(e)) => warning = Some(e.clone()),
            _ => (),
        }
    }
    rsx! {
        div { class: "page-heading", div { span { class: "eyebrow", "VARIETY STARTS HERE" } h2 { "The Indian food library" } p { "Explore dishes before setting your budget. Your food preference is applied; exclusions are marked on each dish." } } }
        div { class: "library-intro", strong { "{c.recipes.len()} dishes · {c.ingredients.len()} ingredients" } span { "{source}" } p { "Breakfasts, rice bowls, rotis, dals and regional favourites. Nutrition is calculated from authored ingredient fixtures; all prices and nutrition are demo estimates." } }
        form { class: "food-search", onsubmit: move |e| { e.prevent_default(); query.set(draft()); offset.set(0); },
            div { label { r#for: "food-query", "Search dishes or cuisines" } input { id: "food-query", r#type: "search", maxlength: "120", placeholder: "Paneer, dosa, Punjabi…", value: "{draft}", oninput: move |e| draft.set(e.value()) } }
            div { label { r#for: "food-meal", "Meal" } select { id: "food-meal", value: "{meal}", onchange: move |e| { meal.set(e.value()); offset.set(0); }, option { value: "", "All meals" } option { value: "breakfast", "Breakfast" } option { value: "lunch", "Lunch" } option { value: "dinner", "Dinner" } } }
            button { class: "primary", r#type: "submit", "Search foods" }
        }
        if let Some(warning) = warning { p { class: "storage-message", role: "status", "{warning}" } }
        p { class: "food-count", role: "status", if !ready && connected() { "Searching database…" } else { "{page.total} matching dishes" } }
        div { class: "food-grid", for recipe in &page.items {
            { let allowed = recipe_allowed(&c, recipe, &p.request()); let cost = recipe_portion_cost(&c, recipe); rsx! {
                article { class: if allowed.is_ok() { "food-card" } else { "food-card excluded" },
                    div { class: "food-card-tags", span { "{recipe.cuisine}" } small { if recipe.vegetarian { "VEG" } else { "MEAT / EGG" } } }
                    h3 { "{recipe.name}" } p { {format!("{} · {}", recipe.category, recipe.meals.iter().map(|m|m.label()).collect::<Vec<_>>().join(" / "))} }
                    if let Some(n) = recipe.nutrition { div { class: "food-macros", strong { "{n.calories_kcal:.0} kcal" } strong { "{n.protein_g:.1} g protein" } } } else { p { "Nutrition unavailable" } }
                    if let Ok((low, high)) = cost { div { class: "food-cost", strong { "{money(low)}–{money(high)}" } small { "Ingredients used / serving · demo" } } }
                    if let Err(reason) = &allowed { p { class: "food-excluded", "Excluded: {reason}" } }
                    details { summary { "Ingredients & preparation" } ul { for a in &recipe.ingredients { if let Some(i) = c.ingredients.iter().find(|i|i.id == a.ingredient) { li { "{a.quantity} {i.unit} {i.name}" } } } } ol { for step in &recipe.steps { li { "{step}" } } } p { "One base serving. Monthly costs combine selected portions and round purchases up to full packs. Preparation time may exclude soaking and fermentation." } }
                }
            } }
        } }
        if page.items.is_empty() { div { class: "no-estimate panel", h3 { "No dishes found" } p { "Try a different food name, cuisine or meal filter." } } }
        div { class: "library-pagination", button { class: "secondary", disabled: offset() == 0, onclick: move |_| { let next = offset().saturating_sub(12); offset.set(next); }, "← Previous" } span { if page.total > 0 && (offset() as u64) < page.total { "{offset()+1}–{(offset() as u64+page.items.len() as u64).min(page.total)} of {page.total}" } else { "0 results on this page" } } button { class: "secondary", disabled: offset() as u64 + 12 >= page.total, onclick: move |_| offset += 12, "Next →" } }
    }
}
