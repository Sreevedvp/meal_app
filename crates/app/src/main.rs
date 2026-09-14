mod planner;
mod storage;
use planner::{Planner, PlannerSession};
mod food_library;
use dioxus::prelude::*;
use food_library::FoodLibrary;
use meal_domain::*;
use serde::{Deserialize, Serialize};
const CSS: Asset = asset!("/assets/style.css");

fn default_api() -> String {
    if let Some(url) = option_env!("MEAL_API_URL") {
        return url.trim_end_matches('/').to_string();
    }
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_default()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        String::new()
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
struct Settings {
    profile: FitnessProfile,
    // Infrastructure is configured by the build, never by stored user preferences.
    #[serde(skip, default = "default_api")]
    endpoint: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            profile: FitnessProfile::default(),
            endpoint: default_api(),
        }
    }
}
fn paise(s: &str) -> Option<u64> {
    let mut parts = s.split('.');
    let whole = parts.next()?.parse::<u64>().ok()?;
    let fraction = parts.next().unwrap_or("");
    if parts.next().is_some() || fraction.len() > 2 || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    whole.checked_mul(100)?.checked_add(if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u64>().ok()? * if fraction.len() == 1 { 10 } else { 1 }
    })
}
async fn connect_catalogue(endpoint: &str) -> Result<Catalogue, String> {
    let url = reqwest::Url::parse(endpoint)
        .map_err(|_| "Enter a complete API URL, including https:// or http://.".to_string())?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Use an HTTP or HTTPS API URL without credentials, a query or a fragment.".into(),
        );
    }
    let response = reqwest::Client::new().get(format!("{}/api/catalogue", endpoint.trim_end_matches('/')))
        .timeout(std::time::Duration::from_secs(8)).send().await
        .map_err(|_| "API unavailable. Your targets and demo budget calculator still work offline. For a phone, use a reachable API address or USB port forwarding.".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "API returned {}. The bundled catalogue is still available.",
            response.status()
        ));
    }
    let c = response
        .json::<Catalogue>()
        .await
        .map_err(|_| "The API returned an unreadable catalogue.".to_string())?;
    if c.version != CATALOGUE_VERSION {
        return Err("The API has a different catalogue version. Restart it with the updated app source; the bundled catalogue is still available.".into());
    }
    Ok(c)
}
fn main() {
    dioxus::launch(App);
}
#[component]
fn App() -> Element {
    let planner_state = use_signal(PlannerSession::default);
    let mut account_error = use_signal(String::new);
    let mut production_account = use_signal(|| false);
    let mut settings = use_signal(Settings::default);
    let mut saved = use_signal(|| None::<Settings>);
    let mut loaded = use_signal(|| false);
    let mut catalogue = use_signal(demo_catalogue);
    let mut period = use_signal(|| 1_u32);
    let mut page = use_signal(|| "targets".to_string());
    let mut suggestions = use_signal(|| false);
    let mut connected = use_signal(|| false);
    let mut connecting = use_signal(|| false);
    let mut storage_message = use_signal(String::new);
    use_resource(move || async move {
        match storage::initialize().await {
            Ok(production) => production_account.set(production),
            Err(e) => {
                account_error.set(e);
                return;
            }
        }
        match document::eval("return window.deccanStorage.getItem('deccan-fitness-v1');")
            .join::<Option<String>>()
            .await
        {
            Ok(Some(json)) => match serde_json::from_str::<Settings>(&json) {
                Ok(value) if value.profile.validate(&demo_catalogue()).is_ok() => {
                    settings.set(value.clone());
                    saved.set(Some(value));
                }
                _ => storage_message
                    .set("Saved settings could not be read. Default targets are shown.".into()),
            },
            Err(_) => storage_message
                .set("Device storage is unavailable. Changes will last for this session.".into()),
            _ => (),
        }
        loaded.set(true);
        // Read once without subscribing: editing settings must not reload saved values.
        let endpoint = settings.peek().endpoint.clone();
        if !endpoint.is_empty() {
            connecting.set(true);
            match connect_catalogue(&endpoint).await {
                Ok(c) => {
                    catalogue.set(c);
                    connected.set(true);
                }
                Err(_) => {
                    connected.set(false);
                }
            }
            connecting.set(false);
        }
    });
    let value = settings();
    let profile = value.profile.clone();
    let daily = profile.daily();
    let weekly = daily.scaled(7.0);
    let monthly = daily.scaled(f64::from(profile.month_days));
    let selected_period = if period() > 7 {
        profile.month_days
    } else {
        period()
    };
    let factor = f64::from(selected_period);
    let period_label = if period() == 1 {
        "day"
    } else if period() == 7 {
        "week"
    } else {
        "month"
    };
    let validation = profile.validate(&catalogue());
    let estimate = use_memo(move || estimate_budget(&catalogue(), &settings().profile));
    let dirty = saved().as_ref() != Some(&value);
    if !account_error().is_empty() {
        return rsx! {
            document::Stylesheet {href:CSS}
            main {class:"account-error panel",h1 {"Sign in to Deccan Table"} p {"{account_error}"} a {class:"primary",href:"/oauth2/sign_in","Sign in again"}}
        };
    }
    rsx! {
        document::Title { "Deccan Table · Fuel your goals" }
        document::Meta { name: "description", content: "Configure calories, protein and a monthly food budget. Fitness nutrition targets that work offline." }
        document::Stylesheet { href: CSS }
        header { class: "topbar",
            a { class: "brand", href: "#", onclick: move |_| page.set("targets".into()), span { class: "brand-mark", "dt" } div { strong { "deccan table" } span { "NUTRITION. WITH A NUMBER." } } }
            if production_account() {button {class:"secondary",onclick:move|_|async move{let _=document::eval("try { window.deccanStorage.clear(); } finally { location.assign('/oauth2/sign_out?rd=/signed-out'); }").await;},"Sign out"}}
            span { class: "connection-pill", span { class: if connected() { "dot online" } else { "dot" } } if connecting() { "Updating…" } else if connected() { "Online" } else { "Offline ready" } }
        }
        main { class: "workspace",
            aside { class: "sidebar",
                div { class: "sidebar-intro", span { class: "eyebrow", "YOUR TRAINING COMPANION" } h1 { "Fuel the work.\nFit the budget." } p { "Make calories, protein and your food budget work together." } }
                nav { "aria-label": "Main navigation",
                    button { class: if page() == "targets" { "nav-item active" } else { "nav-item" }, onclick: move |_| page.set("targets".into()), span { "01" } "Nutrition & budget" }
                    button { class: if page() == "foods" { "nav-item active" } else { "nav-item" }, onclick: move |_| page.set("foods".into()), span { "02" } "Indian food library" }
                    button { class: if page() == "planner" { "nav-item active" } else { "nav-item" }, onclick: move |_| page.set("planner".into()), span { "03" } "Planner" }
                }
                div { class: "sidebar-card", span { class: "eyebrow", "BUILT FOR CONSISTENCY" } h3 { "Your goals. Your call." } p { "Set your own targets. Choose whether nutrition or spending comes first." } }
                p { class: "sidebar-foot", "Hyderabad · INR\nTargets are per person." }
            }
            section { class: "results",
                if page() == "foods" {
                    FoodLibrary { settings, catalogue, connected }
                } else if page() == "planner" {
                    Planner { state: planner_state, settings, catalogue, connected, settings_loaded: loaded }
                } else {
                    div { class: "page-heading", div { span { class: "eyebrow", "MORE INTENTION. EVERY MEAL." } h2 { "Train hard. Eat with a plan." } p { "Start with your numbers. See what they mean for your month." } } span { class: "status-tag subtle", "FITNESS EDITION" } }
                    div { class: "metric-grid",
                        article { class: "metric calories", span { "DAILY CALORIES" } strong { "{daily.calories_kcal:.0}" small { " kcal" } } p { "Across {profile.targets.len()} meals" } }
                        article { class: "metric protein", span { "DAILY PROTEIN" } strong { "{daily.protein_g:.1}" small { " g" } } p { "Your configured target" } }
                        article { class: "metric budget", span { "MONTHLY BUDGET" } strong { "{money(profile.monthly_budget_paise)}" } p { "{money(profile.monthly_budget_paise / u64::from(profile.month_days))} / day · {profile.month_days} days" } }
                    }
                    button { class: "library-banner", onclick: move |_| page.set("foods".into()), div { strong { "Start with the Indian food library" } span { "{catalogue().recipes.len()} dishes · explore ingredients, protein and cost per serving" } } span { "Browse foods →" } }
                    fieldset { class: "configuration", disabled: !loaded(),
                    section { class: "panel", div { class: "panel-heading", div { span { class: "step", "01" } h3 { "Set your nutrition targets" } } span { class: "muted", "Per person" } }
                        div { class: "target-period", label { r#for: "target-period", "Configure targets per" } select { id: "target-period", value: if period()>7 {"month".to_string()} else {period().to_string()}, onchange: move |e| period.set(if e.value()=="month" {settings().profile.month_days} else {e.value().parse().unwrap_or(1)}), option { value: "1", "Day" } option { value: "7", "Week" } option { value: "month", "Month ({profile.month_days} days)" } } }
                        div { class: "input-grid",
                            div { label { r#for: "total-calories", "Calories / {period_label}" } div { class: "unit-input", input { id: "total-calories", r#type: "number", step: "any", min: "0", value: "{daily.calories_kcal * factor:.1}", oninput: move |e| {
                                let total = e.value().parse::<f64>().unwrap_or(0.0) / factor;
                                let mut s = settings.write(); let previous = s.profile.daily().calories_kcal; let count = s.profile.targets.len() as f64;
                                for t in &mut s.profile.targets { t.nutrition.calories_kcal = if previous > 0.0 { total * t.nutrition.calories_kcal / previous } else { total / count }; }
                            } } span { "kcal" } } }
                            div { label { r#for: "total-protein", "Protein / {period_label}" } div { class: "unit-input", input { id: "total-protein", r#type: "number", step: "any", min: "0", value: "{daily.protein_g * factor:.1}", oninput: move |e| {
                                let total = e.value().parse::<f64>().unwrap_or(0.0) / factor;
                                let mut s = settings.write(); let previous = s.profile.daily().protein_g; let count = s.profile.targets.len() as f64;
                                for t in &mut s.profile.targets { t.nutrition.protein_g = if previous > 0.0 { total * t.nutrition.protein_g / previous } else { total / count }; }
                            } } span { "g" } } }
                        }
                        p { class: "hint", "Changing a total scales your meal targets proportionally. Fine-tune each meal below." }
                        div { class: "meal-targets", for (index, target) in profile.targets.iter().enumerate() {
                            article { class: "meal-target", div { class: "meal-title", span { "0{index + 1}" } h4 { "{target.meal.label()}" } }
                                label { r#for: "meal-kcal-{index}", "Calories / meal" } div { class: "unit-input", input { id: "meal-kcal-{index}", r#type: "number", step: "any", min: "50", max: "3000", value: "{target.nutrition.calories_kcal:.1}", oninput: move |e| settings.write().profile.targets[index].nutrition.calories_kcal = e.value().parse().unwrap_or(0.0) } span { "kcal" } }
                                label { r#for: "meal-protein-{index}", "Protein / meal" } div { class: "unit-input", input { id: "meal-protein-{index}", r#type: "number", step: "any", min: "1", max: "250", value: "{target.nutrition.protein_g:.1}", oninput: move |e| settings.write().profile.targets[index].nutrition.protein_g = e.value().parse().unwrap_or(0.0) } span { "g" } }
                            }
                        } }
                        div { class: "period-totals", div { span { "WEEK · 7 DAYS" } strong { "{weekly.calories_kcal:.0} kcal" } small { "{weekly.protein_g:.1} g protein" } } div { span { "MONTH · {profile.month_days} DAYS" } strong { "{monthly.calories_kcal:.0} kcal" } small { "{monthly.protein_g:.1} g protein" } } }
                    }
                    section { class: "panel", div { class: "panel-heading", div { span { class: "step", "02" } h3 { "Give your goals a budget" } } }
                        div { class: "input-grid", div { label { r#for: "budget", "Monthly food budget" } div { class: "unit-input", input { id: "budget", r#type: "number", min: "0.01", max: "1000000", step: "0.01", value: "{profile.monthly_budget_paise as f64 / 100.0:.2}", onchange: move |e| { let text = e.value(); settings.write().profile.monthly_budget_paise = paise(&text).unwrap_or(0); } } span { "INR" } } } div { label { r#for: "month-days", "Days in your month" } select { id: "month-days", value: "{profile.month_days}", onchange: move |e| { let days = e.value().parse().unwrap_or(30); settings.write().profile.month_days = days; if period() > 7 { period.set(days); } }, for days in 28..=31 { option { value: "{days}", "{days} days" } } } } }
                        fieldset { class: "priority-options", legend { "What comes first?" }
                            label { class: if profile.diet_first { "choice selected" } else { "choice" }, input { r#type: "radio", name: "priority", checked: profile.diet_first, onchange: move |_| settings.write().profile.diet_first = true } div { strong { "Diet first" } small { "Show the budget my targets need" } } }
                            label { class: if !profile.diet_first { "choice selected" } else { "choice" }, input { r#type: "radio", name: "priority", checked: !profile.diet_first, onchange: move |_| settings.write().profile.diet_first = false } div { strong { "Budget first" } small { "Help me explore ways to spend less" } } }
                        }
                        div { class: "preference-row", label { r#for: "diet", "Food preference" } select { id: "diet", value: if profile.diet == Diet::Vegetarian { "veg" } else { "meat" }, onchange: move |e| settings.write().profile.diet = if e.value() == "veg" { Diet::Vegetarian } else { Diet::MeatAllowed }, option { value: "veg", "Vegetarian · no meat or eggs" } option { value: "meat", "Meat & eggs allowed" } } }
                        details { class: "exclusions", summary { "Allergies & ingredient exclusions ({profile.allergens.len() + profile.excluded_ingredients.len()})" } div { class: "allergen-grid", for allergen in ALLERGENS { label { input { r#type: "checkbox", checked: profile.allergens.iter().any(|a| a == allergen), onchange: move |e| { let mut s = settings.write(); s.profile.allergens.retain(|a| a != allergen); if e.checked() { s.profile.allergens.push(allergen.to_string()); } } } {allergen.replace('_', " ")} } } }
                            details { summary { "Individual ingredients" } div { class: "allergen-grid", for ingredient in catalogue().ingredients { { let id = ingredient.id.clone(); rsx! { label { input { r#type: "checkbox", checked: profile.excluded_ingredients.contains(&ingredient.id), onchange: move |e| { let mut s = settings.write(); s.profile.excluded_ingredients.retain(|i| i != &id); if e.checked() { s.profile.excluded_ingredients.push(id.clone()); } } } "{ingredient.name}" } } } } } }
                        }
                    }
                    }
                    if let Err(message) = validation { div { class: "error", role: "alert", "{message}" } }
                    section { class: "panel estimate", div { class: "panel-heading", div { span { class: "step", "03" } h3 { if profile.diet_first { "What would your diet cost?" } else { "How does your budget compare?" } } } span { class: "status-tag", "DEMO ESTIMATE" } }
                        match estimate() {
                            Ok(Some(b)) => rsx! {
                                div { class: "estimate-main", div { span { class: "eyebrow", "LOWEST MONTHLY ESTIMATE FOUND" } strong { "{money(b.low_paise)}–{money(b.high_paise)}" } p { "{profile.month_days} days · {b.distinct_recipes} different dishes · full packs" } }
                                    div { class: "budget-comparison", span { if b.high_paise <= profile.monthly_budget_paise { "WITHIN YOUR DEMO BUDGET" } else { "EXTRA TO ALLOW FOR" } } strong { "{money(b.high_paise.abs_diff(profile.monthly_budget_paise))}" } small { if b.high_paise <= profile.monthly_budget_paise { "remaining at the upper estimate" } else { "above your budget at the upper estimate" } } }
                                }
                                if profile.diet_first && b.high_paise > profile.monthly_budget_paise { p { class: "budget-advice", "For these targets, allow {money(b.high_paise)} for the month using the upper demo estimate. Your entered budget and nutrition targets stay yours to change." } }
                                p { class: "estimate-note", "Illustrative nutrition and prices, not verified food data or live quotes. This is the cheapest sampled example found, not a proven minimum. Every selected meal meets your protein minimum and calories within ±10%, with no dish repeated within seven days." }
                                details { class: "estimate-details", summary { "See the varied menu & ingredient costs" } p { "This cost preview selects different dishes across {profile.month_days} days; a dish can return only after seven days. We compare 24 sampled rotations, using 0.5–2 servings in 0.1 steps, then combine actual ingredient quantities before buying full packs. Open the Planner to save a dated calendar, swap meals and organise weekly purchases." }
                                    for example in &b.examples { if let Some(recipe) = catalogue().recipes.iter().find(|r| r.id == example.recipe_id) { div { class: "example-row", div { strong { "Day {example.day + 1} · {example.meal.label()} · {recipe.name}" } small { "{f64::from(example.portion_percent) / 100.0:.1} servings · illustrative nutrition" } } span { "{example.nutrition.calories_kcal:.0} kcal · {example.nutrition.protein_g:.1} g protein" } } } }
                                    div { class: "cost-table", table { thead { tr { th { "Ingredient" } th { "Monthly need" } th { "Full packs" } th { "Estimate" } } } tbody { for g in &b.groceries { tr { td { "{g.name}" } td { "{g.needed} {g.unit}" } td { "{g.packs} × {g.price.pack_quantity} {g.unit}" } td { "{money(g.low_paise)}–{money(g.high_paise)}" } } } } } }
                                    p { "Assumes an empty pantry. Delivery, cooking fuel and food waste are excluded. Ingredients are rounded up per serving; nutrition is calculated from these rounded quantities and authored ingredient fixtures. Estimates do not assess overall dietary adequacy." }
                                }
                            },
                            Ok(None) => rsx! { div { class: "no-estimate", h4 { "No matching estimate yet" } p { "The demo catalogue could not match all meal targets, exclusions and seven days of different dishes within the sampled portions. We cannot calculate a minimum budget for this combination. Your targets have not been reduced." } } },
                            Err(_) => rsx! { p { "Correct the target or budget inputs above to see an estimate." } },
                        }
                        if !profile.diet_first {
                            div { class: "savings-prompt", h4 { "Want suggestions to control spending?" } p { "Explore cost-saving ideas while keeping your calorie and protein goals visible." } button { class: "secondary", "aria-expanded": suggestions(), onclick: move |_| suggestions.toggle(), if suggestions() { "Hide suggestions" } else { "Yes, show suggestions →" } }
                                if suggestions() { ul { li { "Compare local prices for foods already allowed by your preferences and exclusions." } li { "Check what is in your pantry before buying full packs." } li { "Compare pack unit prices and buy only what you can use." } } p { class: "muted", "Open the Planner and choose Swap meal, then Only show cheaper swaps. Weekly shopping costs account for unused packs. No target changes have been applied." } }
                            }
                        }
                    }
                    div { class: "save-row", div { strong { if !loaded() { "Loading saved settings…" } else if dirty { "You have unsaved settings" } else { "Settings saved on this device" } } p { "Your targets are available next time on this device." } }
                        button { class: "primary", disabled: !loaded() || profile.validate(&catalogue()).is_err() || !dirty, onclick: move |_| {
                            let value = settings();
                            spawn(async move {
                                let result = async {
                                    let json = serde_json::to_string(&value).map_err(|e| e.to_string())?;
                                    let literal = serde_json::to_string(&json).map_err(|e| e.to_string())?;
                                    document::eval(&format!("window.deccanStorage.setItem('deccan-fitness-v1', {literal}); return true;" )).join::<bool>().await.map_err(|e|e.to_string())
                                }.await;
                                match result { Ok(true) => { saved.set(Some(value)); storage_message.set("Targets and preferences saved on this device.".into()); }, _ => storage_message.set("Could not save to device storage. Your settings still work for this session.".into()) }
                            });
                        }, "Save my settings →" }
                    }
                    if !storage_message().is_empty() { p { class: "storage-message", role: "status", "{storage_message}" } }
                    button { class: "planner-link", onclick: move |_| page.set("planner".into()), div { strong { "Open your meal & budget planner" } span { "Variety, grocery schedules and money-saving swaps" } } span { class: "status-tag", "Open planner →" } }
                }
                footer { "Demo catalogue · User-defined targets, not nutrition recommendations. Check real food labels, allergens and local prices." }
            }
        }
    }
}
