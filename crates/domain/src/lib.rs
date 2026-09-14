mod planner;
pub use planner::*;
// Shared contracts and deterministic calculations. The API validates every request and swap.
mod fitness;
pub use fitness::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CATALOGUE_VERSION: &str = "india-demo-2026-09-13-v4";
pub const ALLERGENS: &[&str] = &[
    "milk",
    "wheat",
    "peanut",
    "egg",
    "soy",
    "sesame",
    "tree_nut",
    "fish",
    "shellfish",
    "mustard",
];
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Diet {
    Vegetarian,
    MeatAllowed,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Meal {
    Breakfast,
    Lunch,
    Dinner,
}
impl Meal {
    pub fn label(self) -> &'static str {
        match self {
            Self::Breakfast => "Breakfast",
            Self::Lunch => "Lunch",
            Self::Dinner => "Dinner",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlanRequest {
    pub region: String,
    pub adults: u32,
    pub days: u32,
    pub meals: Vec<Meal>,
    pub budget_paise: u64,
    pub diet: Diet,
    pub allergens: Vec<String>,
    pub excluded_ingredients: Vec<String>,
}
impl Default for PlanRequest {
    fn default() -> Self {
        Self {
            region: "Hyderabad".into(),
            adults: 1,
            days: 7,
            meals: vec![Meal::Breakfast, Meal::Lunch, Meal::Dinner],
            budget_paise: 150_000,
            diet: Diet::Vegetarian,
            allergens: vec![],
            excluded_ingredients: vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Price {
    pub kind: String,
    pub region: String,
    pub source: String,
    pub authored_on: String,
    pub observed_at: Option<String>,
    pub retailer: Option<String>,
    pub pack_quantity: u64,
    pub low_paise: u64,
    pub high_paise: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Ingredient {
    #[serde(default)]
    pub nutrition_per_100_units: Option<Nutrition>,
    pub id: String,
    pub name: String,
    pub category: String,
    pub unit: String,
    pub allergens: Option<Vec<String>>,
    pub components: Vec<String>,
    pub price: Price,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Amount {
    pub ingredient: String,
    pub quantity: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Recipe {
    #[serde(default)]
    pub cuisine: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub nutrition: Option<Nutrition>,
    pub id: String,
    pub name: String,
    pub description: String,
    pub meals: Vec<Meal>,
    pub vegetarian: bool,
    pub minutes: u32,
    pub ingredients: Vec<Amount>,
    pub steps: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FoodPage {
    pub total: u64,
    pub items: Vec<Recipe>,
    pub limit: usize,
    pub offset: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Catalogue {
    pub version: String,
    pub ingredients: Vec<Ingredient>,
    pub recipes: Vec<Recipe>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Slot {
    pub day: u32,
    pub meal: Meal,
    pub recipe_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Grocery {
    pub ingredient_id: String,
    pub name: String,
    pub category: String,
    pub unit: String,
    pub needed: u64,
    pub packs: u64,
    pub purchased: u64,
    pub leftover: u64,
    pub low_paise: u64,
    pub high_paise: u64,
    pub price: Price,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Plan {
    pub id: String,
    pub catalogue_version: String,
    pub request: PlanRequest,
    pub slots: Vec<Slot>,
    pub groceries: Vec<Grocery>,
    pub low_paise: u64,
    pub high_paise: u64,
    pub within_estimate: bool,
    pub shortfall_paise: u64,
    pub warnings: Vec<String>,
    pub blocked_recipes: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SwapRequest {
    pub slot_index: usize,
    pub recipe_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub message: String,
}

pub fn money(paise: u64) -> String {
    format!("₹{}.{:02}", paise / 100, paise % 100)
}
pub fn demo_catalogue() -> Catalogue {
    serde_json::from_str(include_str!("catalogue.json")).expect("checked-in catalogue")
}

pub fn validate_request(c: &Catalogue, r: &PlanRequest) -> Result<(), String> {
    if r.region != "Hyderabad" {
        return Err("This pilot supports Hyderabad only.".into());
    }
    if !(1..=12).contains(&r.adults) {
        return Err("Choose 1–12 adults. Child portions are not modelled in this pilot.".into());
    }
    if !(1..=31).contains(&r.days) {
        return Err("Choose 1–31 days.".into());
    }
    if r.meals.is_empty()
        || r.meals.len() > 3
        || r.meals.iter().collect::<BTreeSet<_>>().len() != r.meals.len()
    {
        return Err("Select at least one meal, without duplicates.".into());
    }
    if r.budget_paise == 0 || r.budget_paise > 100_000_000 {
        return Err("Enter a budget from ₹0.01 to ₹10,00,000.".into());
    }
    for a in &r.allergens {
        if !ALLERGENS.contains(&a.as_str()) {
            return Err(format!(
                "Unrecognised allergen '{a}'. Use a supported exclusion or exclude a named ingredient."
            ));
        }
    }
    for id in &r.excluded_ingredients {
        if !c.ingredients.iter().any(|i| &i.id == id) {
            return Err(format!("Unrecognised ingredient exclusion '{id}'."));
        }
    }
    Ok(())
}
fn check_ingredient(
    c: &Catalogue,
    id: &str,
    r: &PlanRequest,
    stack: &mut BTreeSet<String>,
) -> Result<(), String> {
    let i = c
        .ingredients
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| format!("Unknown ingredient {id}"))?;
    if !stack.insert(id.to_string()) {
        return Err(format!("Unresolved compound ingredient: {}", i.name));
    }
    if r.excluded_ingredients.iter().any(|e| e == id) {
        return Err(format!("{} is excluded", i.name));
    }
    let allergens = i
        .allergens
        .as_ref()
        .ok_or_else(|| format!("{} has unreviewed allergen metadata", i.name))?;
    if let Some(a) = allergens.iter().find(|a| !ALLERGENS.contains(&a.as_str())) {
        return Err(format!(
            "{} has unrecognised allergen metadata '{a}'",
            i.name
        ));
    }
    if let Some(a) = allergens.iter().find(|a| r.allergens.contains(a)) {
        return Err(format!("{} contains excluded {a}", i.name));
    }
    for component in &i.components {
        check_ingredient(c, component, r, stack)?;
    }
    stack.remove(id);
    if i.price.region != r.region
        || i.price.pack_quantity == 0
        || i.price.low_paise == 0
        || i.price.low_paise > i.price.high_paise
    {
        return Err(format!("{} has unusable regional price data", i.name));
    }
    Ok(())
}
pub fn recipe_allowed(c: &Catalogue, recipe: &Recipe, r: &PlanRequest) -> Result<(), String> {
    if r.diet == Diet::Vegetarian && !recipe.vegetarian {
        return Err("Meat or egg recipe excluded by vegetarian preference".into());
    }
    if recipe.ingredients.is_empty() {
        return Err("Recipe has no reviewed ingredients".into());
    }
    for a in &recipe.ingredients {
        if a.quantity == 0 {
            return Err("Ingredient quantity is missing".into());
        }
        check_ingredient(c, &a.ingredient, r, &mut BTreeSet::new())?;
    }
    Ok(())
}
pub fn groceries(c: &Catalogue, r: &PlanRequest, slots: &[Slot]) -> Result<Vec<Grocery>, String> {
    let mut totals = BTreeMap::<String, u64>::new();
    for slot in slots {
        let recipe = c
            .recipes
            .iter()
            .find(|x| x.id == slot.recipe_id)
            .ok_or("Unknown recipe")?;
        recipe_allowed(c, recipe, r)?;
        if !recipe.meals.contains(&slot.meal) {
            return Err("Recipe does not match this meal slot".into());
        }
        for amount in &recipe.ingredients {
            *totals.entry(amount.ingredient.clone()).or_default() +=
                amount.quantity * u64::from(r.adults);
        }
    }
    groceries_from_totals(c, totals)
}
fn groceries_from_totals(
    c: &Catalogue,
    totals: BTreeMap<String, u64>,
) -> Result<Vec<Grocery>, String> {
    let mut result = Vec::new();
    for (id, needed) in totals {
        let i = c
            .ingredients
            .iter()
            .find(|i| i.id == id)
            .ok_or("Unknown ingredient")?;
        let packs = needed.div_ceil(i.price.pack_quantity);
        let purchased = packs * i.price.pack_quantity;
        result.push(Grocery {
            ingredient_id: id,
            name: i.name.clone(),
            category: i.category.clone(),
            unit: i.unit.clone(),
            needed,
            packs,
            purchased,
            leftover: purchased - needed,
            low_paise: packs * i.price.low_paise,
            high_paise: packs * i.price.high_paise,
            price: i.price.clone(),
        });
    }
    result.sort_by(|a, b| (&a.category, &a.name).cmp(&(&b.category, &b.name)));
    Ok(result)
}
fn upper_cost(c: &Catalogue, r: &PlanRequest, slots: &[Slot]) -> u64 {
    groceries(c, r, slots)
        .expect("prevalidated recipes")
        .iter()
        .map(|g| g.high_paise)
        .sum()
}
fn finish(c: &Catalogue, r: &PlanRequest, slots: Vec<Slot>) -> Result<Plan, String> {
    validate_request(c, r)?;
    if slots.len() != (r.days as usize * r.meals.len()) {
        return Err("Plan has the wrong number of meals".into());
    }
    let mut seen = BTreeSet::new();
    for s in &slots {
        if s.day >= r.days || !r.meals.contains(&s.meal) || !seen.insert((s.day, s.meal)) {
            return Err("Invalid or repeated meal slot".into());
        }
    }
    let groceries = groceries(c, r, &slots)?;
    let low_paise = groceries.iter().map(|g| g.low_paise).sum();
    let high_paise = groceries.iter().map(|g| g.high_paise).sum();
    let mut warnings = vec!["Demo regional estimates, authored 9 September 2026. No retailer quotes or verified availability. Actual prices may fall outside this range; delivery fees and cooking fuel are excluded.".into(), "Ingredient checks use this catalogue's metadata. Check actual packaging and cross-contact information. This is not a medical-safety or nutritional-adequacy assessment.".into(), "Quantities assume an empty pantry and adult portions. Water is assumed available; all listed seasonings and oil are included in purchases.".into()];
    if high_paise > r.budget_paise {
        warnings.push("No plan within your budget was found. This is the lowest-cost candidate found by a bounded search, not proof that every possible plan is unaffordable. Try fewer meals, a higher budget, or a broader catalogue; exclusions were preserved.".into());
    }
    let blocked_recipes = c
        .recipes
        .iter()
        .filter_map(|recipe| {
            recipe_allowed(c, recipe, r)
                .err()
                .map(|why| format!("{}: {why}", recipe.name))
        })
        .collect();
    Ok(Plan {
        id: String::new(),
        catalogue_version: c.version.clone(),
        request: r.clone(),
        slots,
        groceries,
        low_paise,
        high_paise,
        within_estimate: high_paise <= r.budget_paise,
        shortfall_paise: high_paise.saturating_sub(r.budget_paise),
        warnings,
        blocked_recipes,
    })
}
/// Bounded deterministic candidate search. Pack costs are recalculated for every candidate.
/// Never claims global optimality. Repetition is traded for variety only inside the budget.
pub fn generate(c: &Catalogue, r: &PlanRequest) -> Result<Plan, String> {
    validate_request(c, r)?;
    let options: Vec<Vec<&Recipe>> = r
        .meals
        .iter()
        .map(|m| {
            c.recipes
                .iter()
                .filter(|x| x.meals.contains(m) && recipe_allowed(c, x, r).is_ok())
                .collect()
        })
        .collect();
    for (n, opts) in options.iter().enumerate() {
        if opts.is_empty() {
            return Err(format!(
                "No reviewed {} recipes match your exclusions. Exclusions have not been relaxed.",
                r.meals[n].label().to_lowercase()
            ));
        }
    }
    let mut candidates = Vec::new();
    // Include each stable combination repeated over the period: economical shared staples.
    fn combinations(
        options: &[Vec<&Recipe>],
        at: usize,
        current: &mut Vec<String>,
        out: &mut Vec<Vec<String>>,
    ) {
        if at == options.len() {
            out.push(current.clone());
            return;
        }
        for recipe in &options[at] {
            current.push(recipe.id.clone());
            combinations(options, at + 1, current, out);
            current.pop();
        }
    }
    let mut combos = vec![];
    // Bound the legacy search as the catalogue grows. Fitness costing has its own
    // variety-constrained estimator and does not call this price-only endpoint.
    let economical: Vec<Vec<&Recipe>> = options
        .iter()
        .map(|recipes| {
            let mut recipes = recipes.clone();
            recipes.sort_by_cached_key(|recipe| {
                upper_cost(
                    c,
                    r,
                    &[Slot {
                        day: 0,
                        meal: recipe.meals[0],
                        recipe_id: recipe.id.clone(),
                    }],
                )
            });
            recipes.truncate(6);
            recipes
        })
        .collect();
    combinations(&economical, 0, &mut vec![], &mut combos);
    for combo in combos {
        candidates.push(
            (0..r.days)
                .flat_map(|day| {
                    r.meals
                        .iter()
                        .enumerate()
                        .map(|(m, meal)| Slot {
                            day,
                            meal: *meal,
                            recipe_id: combo[m].clone(),
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>(),
        );
    }
    // Rotations provide variety without using random output or an LLM.
    for shift in 0..12 {
        candidates.push(
            (0..r.days)
                .flat_map(|day| {
                    r.meals
                        .iter()
                        .enumerate()
                        .map(|(m, meal)| Slot {
                            day,
                            meal: *meal,
                            recipe_id: options[m][(day as usize + shift + m) % options[m].len()]
                                .id
                                .clone(),
                        })
                        .collect::<Vec<_>>()
                })
                .collect(),
        );
    }
    candidates.sort_by_cached_key(|s| upper_cost(c, r, s));
    let mut best = candidates.first().ok_or("No candidates")?.clone();
    // Choose the most diverse affordable candidate; lower pack cost breaks ties.
    let mut diversity = 0;
    for candidate in &candidates {
        if upper_cost(c, r, candidate) <= r.budget_paise {
            let distinct = candidate
                .iter()
                .map(|s| &s.recipe_id)
                .collect::<BTreeSet<_>>()
                .len();
            if distinct > diversity {
                best = candidate.clone();
                diversity = distinct;
            }
        }
    }
    finish(c, r, best)
}
pub fn swap(c: &Catalogue, original: &Plan, index: usize, recipe_id: &str) -> Result<Plan, String> {
    validate_request(c, &original.request)?;
    if original.catalogue_version != c.version {
        return Err("Catalogue changed; generate a fresh plan".into());
    }
    let mut slots = original.slots.clone();
    let slot = slots.get_mut(index).ok_or("Unknown meal slot")?;
    if slot.recipe_id == recipe_id {
        return Err("Choose a different recipe".into());
    }
    slot.recipe_id = recipe_id.to_string();
    let updated = finish(c, &original.request, slots)?;
    if !updated.within_estimate {
        return Err(format!(
            "Swap exceeds your budget at the upper demo estimate by {}. Your existing plan is unchanged.",
            money(updated.shortfall_paise)
        ));
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn misspelled_allergen_metadata_is_not_accepted() {
        let mut c = demo_catalogue();
        c.ingredients
            .iter_mut()
            .find(|i| i.id == "peanut")
            .unwrap()
            .allergens = Some(vec!["peanuts".into()]);
        let r = PlanRequest::default();
        let recipe = c.recipes.iter().find(|r| r.id == "peanut-poha").unwrap();
        assert!(
            recipe_allowed(&c, recipe, &r)
                .unwrap_err()
                .contains("unrecognised")
        );
    }
    #[test]
    fn cyclic_compounds_are_not_accepted() {
        let mut c = demo_catalogue();
        c.ingredients
            .iter_mut()
            .find(|i| i.id == "peanut")
            .unwrap()
            .components = vec!["peanut".into()];
        let recipe = c.recipes.iter().find(|r| r.id == "peanut-poha").unwrap();
        assert!(
            recipe_allowed(&c, recipe, &PlanRequest::default())
                .unwrap_err()
                .contains("Unresolved")
        );
    }
    #[test]
    fn default_plan_is_complete_and_pack_costed() {
        let c = demo_catalogue();
        let r = PlanRequest::default();
        let p = generate(&c, &r).unwrap();
        assert_eq!(p.slots.len(), 21);
        assert!(p.within_estimate);
        assert_eq!(
            p.high_paise,
            p.groceries.iter().map(|g| g.high_paise).sum::<u64>()
        );
        for g in p.groceries {
            assert!(g.purchased >= g.needed);
            assert_eq!(g.purchased, g.packs * g.price.pack_quantity);
            assert_eq!(g.high_paise, g.packs * g.price.high_paise);
        }
    }
    #[test]
    fn household_scales_demand_before_pack_rounding() {
        let c = demo_catalogue();
        let mut r = PlanRequest::default();
        let p = generate(&c, &r).unwrap();
        let one = groceries(&c, &r, &p.slots).unwrap();
        r.adults = 3;
        let three = groceries(&c, &r, &p.slots).unwrap();
        for (a, b) in one.iter().zip(three) {
            assert_eq!(b.needed, a.needed * 3);
            assert!(b.high_paise <= a.high_paise * 3);
        }
    }
    #[test]
    fn tiny_budget_is_not_presented_as_feasible() {
        let c = demo_catalogue();
        let r = PlanRequest {
            budget_paise: 1,
            ..Default::default()
        };
        let p = generate(&c, &r).unwrap();
        assert!(!p.within_estimate);
        assert_eq!(p.shortfall_paise, p.high_paise - 1);
    }
    #[test]
    fn unknown_compound_and_nested_allergens_fail_closed() {
        let mut c = demo_catalogue();
        let mut r = PlanRequest::default();
        let unknown = c.recipes.iter().find(|x| x.id == "mix-rice").unwrap();
        assert!(recipe_allowed(&c, unknown, &r).is_err());
        let peanut = c.ingredients.iter_mut().find(|x| x.id == "peanut").unwrap();
        peanut.components = vec!["milk".into()];
        r.allergens = vec!["milk".into()];
        let poha = c.recipes.iter().find(|x| x.id == "peanut-poha").unwrap();
        assert!(recipe_allowed(&c, poha, &r).is_err());
    }
    #[test]
    fn excluded_ingredients_never_reappear_in_plan_or_swap() {
        let c = demo_catalogue();
        let r = PlanRequest {
            allergens: vec!["milk".into(), "peanut".into(), "wheat".into()],
            ..Default::default()
        };
        let p = generate(&c, &r).unwrap();
        assert!(
            !p.groceries
                .iter()
                .any(|g| ["milk", "peanut", "atta"].contains(&g.ingredient_id.as_str()))
        );
        assert!(swap(&c, &p, 0, "peanut-poha").is_err());
        assert!(swap(&c, &p, 0, "mix-rice").is_err());
    }
    #[test]
    fn no_matching_recipe_reports_failure_without_relaxing_exclusions() {
        let c = demo_catalogue();
        let r = PlanRequest {
            excluded_ingredients: c.ingredients.iter().map(|i| i.id.clone()).collect(),
            ..Default::default()
        };
        assert!(generate(&c, &r).is_err());
    }
    #[test]
    fn successful_swap_recalculates_and_preserves_other_slots() {
        let c = demo_catalogue();
        let r = PlanRequest {
            budget_paise: 1_000_000,
            ..Default::default()
        };
        let p = generate(&c, &r).unwrap();
        let id = if p.slots[0].recipe_id == "peanut-poha" {
            "vegetable-upma"
        } else {
            "peanut-poha"
        };
        let next = swap(&c, &p, 0, id).unwrap();
        assert_ne!(p.slots[0], next.slots[0]);
        assert_eq!(p.slots[1..], next.slots[1..]);
        assert_eq!(next.groceries, groceries(&c, &r, &next.slots).unwrap());
    }
    #[test]
    fn malformed_request_and_slot_are_rejected() {
        let c = demo_catalogue();
        let mut r = PlanRequest {
            adults: 0,
            ..Default::default()
        };
        assert!(generate(&c, &r).is_err());
        r.adults = 1;
        r.allergens = vec!["unknown".into()];
        assert!(generate(&c, &r).is_err());
        let p = generate(&c, &PlanRequest::default()).unwrap();
        assert!(swap(&c, &p, 100, "peanut-poha").is_err());
    }
    #[test]
    fn swap_cannot_bypass_budget() {
        let c = demo_catalogue();
        let mut p = generate(&c, &PlanRequest::default()).unwrap();
        p.request.budget_paise = 1;
        assert!(swap(&c, &p, 0, "vegetable-upma").is_err());
    }
    #[test]
    fn longer_period_and_selected_meals_work() {
        let c = demo_catalogue();
        let r = PlanRequest {
            days: 28,
            meals: vec![Meal::Dinner],
            budget_paise: 1_000_000,
            ..Default::default()
        };
        assert_eq!(generate(&c, &r).unwrap().slots.len(), 28);
    }
}
