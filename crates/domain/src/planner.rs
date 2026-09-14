//! Fitness calendars use the same constraints offline and at database save time.
use crate::*;
use time::{Date, Duration, Month};

#[derive(Clone, Debug)]
pub struct CalendarCell {
    pub date: String,
    pub number: u8,
    pub plan_day: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct CalendarMonth {
    pub title: String,
    pub page: u32,
    pub pages: u32,
    pub cells: Vec<Option<CalendarCell>>,
}

/// A Monday-first month grid, including disabled dates outside the plan.
pub fn calendar_month(start: &str, days: u32, page: u32) -> Result<CalendarMonth, String> {
    if !(1..=31).contains(&days) {
        return Err("Calendar length must be 1–31 days.".into());
    }
    let start = parse_plan_date(start)?;
    let end = start + Duration::days(i64::from(days - 1));
    let month_index = |d: Date| d.year() * 12 + i32::from(d.month() as u8) - 1;
    let pages = (month_index(end) - month_index(start) + 1) as u32;
    let page = page.min(pages - 1);
    let index = month_index(start) + page as i32;
    let month = Month::try_from((index % 12 + 1) as u8).map_err(|e| e.to_string())?;
    let first = Date::from_calendar_date(index / 12, month, 1).map_err(|e| e.to_string())?;
    let mut cells = vec![None; usize::from(first.weekday().number_days_from_monday())];
    let mut date = first;
    while date.month() == month {
        cells.push(Some(CalendarCell {
            date: date.to_string(),
            number: date.day(),
            plan_day: (date >= start && date <= end).then(|| (date - start).whole_days() as u32),
        }));
        date += Duration::days(1);
    }
    cells.resize(cells.len().div_ceil(7) * 7, None);
    Ok(CalendarMonth {
        title: format!("{month} {}", first.year()),
        page,
        pages,
        cells,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PlannerRequest {
    pub profile: FitnessProfile,
    pub start_date: String,
    pub days: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FitnessPlan {
    pub id: String,
    pub catalogue_version: String,
    pub request: PlannerRequest,
    pub meals: Vec<CostExample>,
    pub groceries: Vec<Grocery>,
    pub low_paise: u64,
    pub high_paise: u64,
    pub budget_paise: u64,
    pub nutrition: Nutrition,
    pub shopping_weeks: Vec<ShoppingWeek>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ShoppingWeek {
    pub first_day: u32,
    pub days: u32,
    pub items: Vec<WeeklyPurchase>,
    pub low_paise: u64,
    pub high_paise: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WeeklyPurchase {
    pub ingredient_id: String,
    pub name: String,
    pub unit: String,
    pub needed: u64,
    pub carry_in: u64,
    pub packs: u64,
    pub buy_quantity: u64,
    pub leftover: u64,
    pub low_paise: u64,
    pub high_paise: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlannerSaveRequest {
    #[serde(default)]
    pub revision: Option<u64>,
    pub request: PlannerRequest,
    pub meals: Vec<CostExample>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StoredFitnessPlan {
    #[serde(default = "initial_revision")]
    pub revision: u64,
    pub plan: FitnessPlan,
    pub catalogue: Catalogue,
}
fn initial_revision() -> u64 {
    1
}
#[derive(Clone, Debug, PartialEq)]
pub struct FitnessSwap {
    pub recipe_id: String,
    pub portion_percent: u32,
    pub nutrition: Nutrition,
    pub high_paise: u64,
}
pub fn parse_plan_date(value: &str) -> Result<Date, String> {
    let error = || "Choose a valid start date between 2000 and 2100 (YYYY-MM-DD).".to_string();
    if value.len() != 10 {
        return Err(error());
    }
    let parts: Vec<_> = value.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return Err(error());
    }
    let year: i32 = parts[0].parse().map_err(|_| error())?;
    let month: u8 = parts[1].parse().map_err(|_| error())?;
    let day: u8 = parts[2].parse().map_err(|_| error())?;
    if !(2000..=2100).contains(&year) {
        return Err(error());
    }
    Date::from_calendar_date(year, Month::try_from(month).map_err(|_| error())?, day)
        .map_err(|_| error())
}
pub fn plan_date(start: &str, day: u32) -> String {
    parse_plan_date(start)
        .ok()
        .and_then(|d| d.checked_add(Duration::days(i64::from(day))))
        .map(|d| d.to_string())
        .unwrap_or_default()
}
fn validate_planner(c: &Catalogue, r: &PlannerRequest) -> Result<(), String> {
    r.profile.validate(c)?;
    parse_plan_date(&r.start_date)?;
    if r.days != 7 && r.days != r.profile.month_days {
        return Err("Choose seven days or the configured month length.".into());
    }
    Ok(())
}
pub fn portion_nutrition(
    c: &Catalogue,
    recipe: &Recipe,
    percent: u32,
) -> Result<Nutrition, String> {
    if !(50..=200).contains(&percent) || !percent.is_multiple_of(10) {
        return Err("Choose a portion between 0.5 and 2 servings, in 0.1 steps.".into());
    }
    if recipe.nutrition.is_none() {
        return Err("Recipe nutrition is unavailable.".into());
    }
    let mut total = Nutrition::default();
    for a in &recipe.ingredients {
        let i = c
            .ingredients
            .iter()
            .find(|i| i.id == a.ingredient)
            .ok_or("Unknown ingredient")?;
        let n = i
            .nutrition_per_100_units
            .ok_or("Ingredient nutrition is unavailable")?;
        if !n.calories_kcal.is_finite()
            || !n.protein_g.is_finite()
            || n.calories_kcal < 0.0
            || n.protein_g < 0.0
        {
            return Err("Ingredient nutrition is invalid.".into());
        }
        let quantity = (a.quantity * u64::from(percent)).div_ceil(100);
        total.calories_kcal += n.calories_kcal * quantity as f64 / 100.0;
        total.protein_g += n.protein_g * quantity as f64 / 100.0;
    }
    Ok(total)
}
pub fn generate_fitness_plan(c: &Catalogue, r: &PlannerRequest) -> Result<FitnessPlan, String> {
    validate_planner(c, r)?;
    let estimate = estimate_for_days(c, &r.profile, r.days)?.ok_or("No varied plan found for these targets and exclusions. Try adjusting your targets or food choices; no constraints were relaxed.")?;
    finish_fitness_plan(c, r, estimate.examples)
}
/// Recalculate client-supplied meals from authoritative ingredient quantities and prices.
pub fn finish_fitness_plan(
    c: &Catalogue,
    r: &PlannerRequest,
    mut meals: Vec<CostExample>,
) -> Result<FitnessPlan, String> {
    validate_planner(c, r)?;
    if meals.len() != r.days as usize * r.profile.targets.len() {
        return Err("The calendar has missing or extra meals.".into());
    }
    meals.sort_by_key(|m| (m.day, m.meal));
    let mut slots = std::collections::BTreeSet::new();
    let mut last = std::collections::BTreeMap::new();
    let mut nutrition = Nutrition::default();
    for meal in &mut meals {
        if meal.day >= r.days || !slots.insert((meal.day, meal.meal)) {
            return Err("Invalid or duplicate calendar slot.".into());
        }
        let target = r
            .profile
            .targets
            .iter()
            .find(|t| t.meal == meal.meal)
            .ok_or("Unexpected meal type")?;
        let recipe = c
            .recipes
            .iter()
            .find(|x| x.id == meal.recipe_id)
            .ok_or("Unknown dish")?;
        recipe_allowed(c, recipe, &r.profile.request())?;
        if !recipe.meals.contains(&meal.meal) {
            return Err("Dish does not fit this meal type.".into());
        }
        if let Some(previous) = last.insert(&meal.recipe_id, meal.day)
            && meal.day < previous + 7
        {
            return Err("Keep at least seven days between repeats of the same dish.".into());
        }
        meal.nutrition = portion_nutrition(c, recipe, meal.portion_percent)?;
        let n = meal.nutrition;
        if n.protein_g + 0.0001 < target.nutrition.protein_g
            || n.calories_kcal + 0.0001 < target.nutrition.calories_kcal * 0.9
            || n.calories_kcal > target.nutrition.calories_kcal * 1.1 + 0.0001
        {
            return Err(format!(
                "{} on day {} does not meet the calorie/protein target at this portion.",
                recipe.name,
                meal.day + 1
            ));
        }
        nutrition.calories_kcal += n.calories_kcal;
        nutrition.protein_g += n.protein_g;
    }
    let groceries = cost_examples(c, &meals)?;
    let low_paise = groceries.iter().map(|g| g.low_paise).sum();
    let high_paise = groceries.iter().map(|g| g.high_paise).sum::<u64>();
    let budget_paise =
        r.profile.monthly_budget_paise * u64::from(r.days) / u64::from(r.profile.month_days);
    if !r.profile.diet_first && high_paise > budget_paise {
        return Err(format!(
            "No plan found within this period's budget of {}. This candidate needs {} at the upper demo estimate. Raise the budget or choose Diet first to view it; your nutrition targets were preserved.",
            money(budget_paise),
            money(high_paise)
        ));
    }
    let shopping_weeks = shopping_weeks(c, &meals, r.days)?;
    Ok(FitnessPlan {
        id: String::new(),
        catalogue_version: c.version.clone(),
        request: r.clone(),
        meals,
        groceries,
        low_paise,
        high_paise,
        budget_paise,
        nutrition,
        shopping_weeks,
    })
}
fn shopping_weeks(
    c: &Catalogue,
    meals: &[CostExample],
    days: u32,
) -> Result<Vec<ShoppingWeek>, String> {
    let mut inventory = std::collections::BTreeMap::<String, u64>::new();
    let mut weeks = vec![];
    for first_day in (0..days).step_by(7) {
        let selections: Vec<_> = meals
            .iter()
            .filter(|m| m.day >= first_day && m.day < first_day + 7)
            .cloned()
            .collect();
        let mut items = vec![];
        for g in cost_examples(c, &selections)? {
            let carry_in = *inventory.get(&g.ingredient_id).unwrap_or(&0);
            let packs = g
                .needed
                .saturating_sub(carry_in)
                .div_ceil(g.price.pack_quantity);
            let buy_quantity = packs * g.price.pack_quantity;
            let leftover = carry_in + buy_quantity - g.needed;
            inventory.insert(g.ingredient_id.clone(), leftover);
            items.push(WeeklyPurchase {
                ingredient_id: g.ingredient_id,
                name: g.name,
                unit: g.unit,
                needed: g.needed,
                carry_in,
                packs,
                buy_quantity,
                leftover,
                low_paise: packs * g.price.low_paise,
                high_paise: packs * g.price.high_paise,
            });
        }
        weeks.push(ShoppingWeek {
            first_day,
            days: (days - first_day).min(7),
            low_paise: items.iter().map(|i| i.low_paise).sum(),
            high_paise: items.iter().map(|i| i.high_paise).sum(),
            items,
        });
    }
    Ok(weeks)
}
pub fn swap_fitness_meal(
    c: &Catalogue,
    p: &FitnessPlan,
    index: usize,
    recipe_id: &str,
    percent: u32,
) -> Result<FitnessPlan, String> {
    if c.version != p.catalogue_version {
        return Err("Use this plan's saved catalogue for swaps.".into());
    }
    let mut meals = p.meals.clone();
    let meal = meals.get_mut(index).ok_or("Unknown meal slot")?;
    if meal.recipe_id == recipe_id && meal.portion_percent == percent {
        return Err("Choose a different meal or portion.".into());
    }
    meal.recipe_id = recipe_id.into();
    meal.portion_percent = percent;
    finish_fitness_plan(c, &p.request, meals)
}
pub fn fitness_swap_options(c: &Catalogue, p: &FitnessPlan, index: usize) -> Vec<FitnessSwap> {
    let Some(slot) = p.meals.get(index) else {
        return vec![];
    };
    let Some(target) = p
        .request
        .profile
        .targets
        .iter()
        .find(|t| t.meal == slot.meal)
    else {
        return vec![];
    };
    let mut options = vec![];
    for recipe in &c.recipes {
        if recipe.id == slot.recipe_id
            || !recipe.meals.contains(&slot.meal)
            || recipe_allowed(c, recipe, &p.request.profile.request()).is_err()
        {
            continue;
        }
        if p.meals
            .iter()
            .enumerate()
            .any(|(i, m)| i != index && m.recipe_id == recipe.id && m.day.abs_diff(slot.day) < 7)
        {
            continue;
        }
        for percent in (50..=200).step_by(10) {
            let Ok(n) = portion_nutrition(c, recipe, percent) else {
                continue;
            };
            if n.protein_g < target.nutrition.protein_g
                || n.calories_kcal < target.nutrition.calories_kcal * 0.9
                || n.calories_kcal > target.nutrition.calories_kcal * 1.1
            {
                continue;
            }
            if let Ok(next) = swap_fitness_meal(c, p, index, &recipe.id, percent) {
                options.push(FitnessSwap {
                    recipe_id: recipe.id.clone(),
                    portion_percent: percent,
                    nutrition: n,
                    high_paise: next.high_paise,
                });
                break;
            }
        }
    }
    options.sort_by(|a, b| (a.high_paise, &a.recipe_id).cmp(&(b.high_paise, &b.recipe_id)));
    options
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(days: u32) -> PlannerRequest {
        PlannerRequest {
            profile: FitnessProfile::default(),
            start_date: "2026-09-28".into(),
            days,
        }
    }
    #[test]
    fn calendar_dates_cross_month_year_and_leap_day() {
        assert_eq!(plan_date("2024-02-28", 1), "2024-02-29");
        assert_eq!(plan_date("2026-12-31", 1), "2027-01-01");
        assert!(parse_plan_date("2026-02-29").is_err());
        assert!(parse_plan_date("2026-2-01").is_err());
    }
    #[test]
    fn weekly_and_monthly_plans_reconcile_nutrition_and_shopping() {
        let c = demo_catalogue();
        for days in [7, 30] {
            let p = generate_fitness_plan(&c, &request(days)).unwrap();
            assert_eq!(p.meals.len(), days as usize * 3);
            assert_eq!(p.budget_paise, 600_000 * u64::from(days) / 30);
            assert_eq!(
                p.high_paise,
                p.shopping_weeks.iter().map(|w| w.high_paise).sum::<u64>()
            );
            assert_eq!(
                p.low_paise,
                p.shopping_weeks.iter().map(|w| w.low_paise).sum::<u64>()
            );
            assert_eq!(
                p.shopping_weeks.last().unwrap().days,
                if days == 7 { 7 } else { 2 }
            );
            assert_eq!(
                p.nutrition.protein_g,
                p.meals.iter().map(|m| m.nutrition.protein_g).sum::<f64>()
            );
            let mut need = std::collections::BTreeMap::<String, u64>::new();
            for week in &p.shopping_weeks {
                for item in &week.items {
                    assert_eq!(
                        item.carry_in + item.buy_quantity,
                        item.needed + item.leftover
                    );
                    *need.entry(item.ingredient_id.clone()).or_default() += item.needed;
                }
            }
            for g in &p.groceries {
                assert_eq!(need[&g.ingredient_id], g.needed);
            }
        }
    }
    #[test]
    fn tampered_nutrition_is_recomputed_and_duplicate_slots_rejected() {
        let c = demo_catalogue();
        let p = generate_fitness_plan(&c, &request(7)).unwrap();
        let mut meals = p.meals.clone();
        meals[0].nutrition.protein_g = 999.0;
        assert_eq!(finish_fitness_plan(&c, &p.request, meals).unwrap(), p);
        let mut meals = p.meals.clone();
        meals[1] = meals[0].clone();
        assert!(finish_fitness_plan(&c, &p.request, meals).is_err());
    }
    #[test]
    fn swaps_preserve_other_meals_and_enforce_all_constraints() {
        let c = demo_catalogue();
        let p = generate_fitness_plan(&c, &request(30)).unwrap();
        let (index, option) = (0..p.meals.len())
            .find_map(|i| {
                fitness_swap_options(&c, &p, i)
                    .first()
                    .cloned()
                    .map(|o| (i, o))
            })
            .unwrap();
        let next =
            swap_fitness_meal(&c, &p, index, &option.recipe_id, option.portion_percent).unwrap();
        for (i, m) in p.meals.iter().enumerate() {
            if i != index {
                assert_eq!(m, &next.meals[i]);
            }
        }
        assert_eq!(next.high_paise, option.high_paise);
        assert!(swap_fitness_meal(&c, &p, 0, "mix-rice", 100).is_err());
        assert!(
            swap_fitness_meal(&c, &p, 1, &p.meals[2].recipe_id, p.meals[2].portion_percent)
                .is_err()
        );
        assert!(swap_fitness_meal(&c, &p, 0, &p.meals[0].recipe_id, 999).is_err());
    }
    #[test]
    fn budget_priority_cannot_be_bypassed() {
        let c = demo_catalogue();
        let mut r = request(7);
        r.profile.monthly_budget_paise = 1;
        assert!(generate_fitness_plan(&c, &r).unwrap().high_paise > 0);
        r.profile.diet_first = false;
        assert!(
            generate_fitness_plan(&c, &r)
                .unwrap_err()
                .contains("budget")
        );
    }
    #[test]
    fn whole_eggs_use_the_actual_rounded_quantity_for_nutrition() {
        let mut c = demo_catalogue();
        let mut r = c
            .recipes
            .iter()
            .find(|r| r.id == "egg-roti")
            .unwrap()
            .clone();
        r.ingredients = vec![Amount {
            ingredient: "egg".into(),
            quantity: 3,
        }];
        c.recipes = vec![r.clone()];
        let n = portion_nutrition(&c, &r, 50).unwrap();
        assert!((n.protein_g - 12.6).abs() < 0.001);
        assert!((n.calories_kcal - 144.0).abs() < 0.001);
    }
}

#[cfg(test)]
mod calendar_tests {
    use super::*;

    #[test]
    fn month_grid_aligns_weekdays_and_crosses_years() {
        let december = calendar_month("2026-12-28", 7, 0).unwrap();
        assert_eq!(december.title, "December 2026");
        assert_eq!(december.pages, 2);
        assert!(december.cells[0].is_none()); // December 1 is Tuesday.
        assert_eq!(december.cells[28].as_ref().unwrap().plan_day, Some(0));
        let january = calendar_month("2026-12-28", 7, 99).unwrap();
        assert_eq!(january.page, 1);
        assert_eq!(january.title, "January 2027");
        assert_eq!(january.cells[4].as_ref().unwrap().plan_day, Some(4));
        assert_eq!(january.cells[6].as_ref().unwrap().plan_day, Some(6));
        assert_eq!(january.cells[7].as_ref().unwrap().plan_day, None);
    }

    #[test]
    fn leap_month_and_three_month_span_keep_every_plan_day() {
        for (start, days, pages) in [("2028-02-01", 29, 1), ("2027-01-31", 31, 3)] {
            let mut offsets = vec![];
            for page in 0..pages {
                let grid = calendar_month(start, days, page).unwrap();
                assert_eq!(grid.pages, pages);
                assert_eq!(grid.cells.len() % 7, 0);
                offsets.extend(grid.cells.iter().flatten().filter_map(|c| c.plan_day));
            }
            assert_eq!(offsets, (0..days).collect::<Vec<_>>());
        }
        assert!(calendar_month("bad", 7, 0).is_err());
        assert!(calendar_month("2026-09-01", 0, 0).is_err());
    }
}
