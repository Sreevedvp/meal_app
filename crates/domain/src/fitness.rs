//! Shared target arithmetic and varied candidate search for previews and calendars.
use crate::*;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Nutrition {
    pub calories_kcal: f64,
    pub protein_g: f64,
}
impl Nutrition {
    pub fn scaled(self, factor: f64) -> Self {
        Self {
            calories_kcal: self.calories_kcal * factor,
            protein_g: self.protein_g * factor,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MealTarget {
    pub meal: Meal,
    pub nutrition: Nutrition,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FitnessProfile {
    pub month_days: u32,
    pub monthly_budget_paise: u64,
    pub diet_first: bool,
    pub targets: Vec<MealTarget>,
    pub diet: Diet,
    pub allergens: Vec<String>,
    pub excluded_ingredients: Vec<String>,
}
impl Default for FitnessProfile {
    fn default() -> Self {
        Self {
            month_days: 30,
            monthly_budget_paise: 600_000,
            diet_first: true,
            targets: vec![
                MealTarget {
                    meal: Meal::Breakfast,
                    nutrition: Nutrition {
                        calories_kcal: 600.0,
                        protein_g: 30.0,
                    },
                },
                MealTarget {
                    meal: Meal::Lunch,
                    nutrition: Nutrition {
                        calories_kcal: 800.0,
                        protein_g: 45.0,
                    },
                },
                MealTarget {
                    meal: Meal::Dinner,
                    nutrition: Nutrition {
                        calories_kcal: 700.0,
                        protein_g: 45.0,
                    },
                },
            ],
            diet: Diet::Vegetarian,
            allergens: vec![],
            excluded_ingredients: vec![],
        }
    }
}
impl FitnessProfile {
    pub fn daily(&self) -> Nutrition {
        self.targets
            .iter()
            .fold(Nutrition::default(), |mut total, t| {
                total.calories_kcal += t.nutrition.calories_kcal;
                total.protein_g += t.nutrition.protein_g;
                total
            })
    }
    pub fn request(&self) -> PlanRequest {
        PlanRequest {
            days: self.month_days,
            adults: 1,
            meals: self.targets.iter().map(|t| t.meal).collect(),
            budget_paise: self.monthly_budget_paise,
            diet: self.diet.clone(),
            allergens: self.allergens.clone(),
            excluded_ingredients: self.excluded_ingredients.clone(),
            ..Default::default()
        }
    }
    pub fn validate(&self, c: &Catalogue) -> Result<(), String> {
        validate_request(c, &self.request())?;
        if !(28..=31).contains(&self.month_days) {
            return Err("Choose a month with 28–31 days.".into());
        }
        for t in &self.targets {
            let n = t.nutrition;
            if !n.calories_kcal.is_finite()
                || !(50.0..=3000.0).contains(&n.calories_kcal)
                || !n.protein_g.is_finite()
                || !(1.0..=250.0).contains(&n.protein_g)
            {
                return Err("Each meal needs 50–3,000 kcal and 1–250 g protein.".into());
            }
            if n.protein_g * 4.0 > n.calories_kcal {
                return Err(format!(
                    "{}: protein alone exceeds this meal's calorie target.",
                    t.meal.label()
                ));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CostExample {
    pub day: u32,
    pub meal: Meal,
    pub recipe_id: String,
    pub portion_percent: u32,
    pub nutrition: Nutrition,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BudgetEstimate {
    pub low_paise: u64,
    pub high_paise: u64,
    pub examples: Vec<CostExample>,
    pub groceries: Vec<Grocery>,
    pub distinct_recipes: usize,
}
/// Cost 24 sampled, varied monthly menus. Each dish has a seven-day cooldown,
/// with each meal meeting its calorie range and protein minimum. No repeated-day
/// fallback is allowed when the catalogue cannot support these constraints.
pub fn estimate_budget(
    c: &Catalogue,
    p: &FitnessProfile,
) -> Result<Option<BudgetEstimate>, String> {
    estimate_for_days(c, p, p.month_days)
}
pub fn estimate_for_days(
    c: &Catalogue,
    p: &FitnessProfile,
    days: u32,
) -> Result<Option<BudgetEstimate>, String> {
    p.validate(c)?;
    if !(1..=31).contains(&days) {
        return Err("Choose 1–31 days.".into());
    }
    let request = p.request();
    let mut options = Vec::new();
    for target in &p.targets {
        let mut meals = Vec::new();
        for recipe in c
            .recipes
            .iter()
            .filter(|r| r.meals.contains(&target.meal) && recipe_allowed(c, r, &request).is_ok())
        {
            let Some(n) = recipe.nutrition else { continue };
            if !n.calories_kcal.is_finite()
                || !n.protein_g.is_finite()
                || n.calories_kcal <= 0.0
                || n.protein_g < 0.0
            {
                continue;
            }
            for percent in (50..=200).step_by(10) {
                let Ok(nutrition) = portion_nutrition(c, recipe, percent) else {
                    continue;
                };
                if nutrition.calories_kcal >= target.nutrition.calories_kcal * 0.9
                    && nutrition.calories_kcal <= target.nutrition.calories_kcal * 1.1
                    && nutrition.protein_g >= target.nutrition.protein_g
                {
                    meals.push(CostExample {
                        day: 0,
                        meal: target.meal,
                        recipe_id: recipe.id.clone(),
                        portion_percent: percent,
                        nutrition,
                    });
                }
            }
        }
        if meals.is_empty() {
            return Ok(None);
        }
        options.push(meals);
    }
    // Keep the cheapest valid portion of each dish. Larger portions cannot lower
    // ingredient demand; every retained portion already satisfies both targets.
    for choices in &mut options {
        let mut seen = BTreeSet::new();
        choices.retain(|e| seen.insert(e.recipe_id.clone()));
        choices.sort_by_cached_key(|e| {
            let recipe = c.recipes.iter().find(|r| r.id == e.recipe_id).unwrap();
            let cost = recipe
                .ingredients
                .iter()
                .map(|a| {
                    let i = c.ingredients.iter().find(|i| i.id == a.ingredient).unwrap();
                    let quantity = (a.quantity * u64::from(e.portion_percent)).div_ceil(100);
                    (quantity * i.price.high_paise).div_ceil(i.price.pack_quantity)
                })
                .sum::<u64>();
            (cost, e.recipe_id.clone())
        });
    }
    // Assign the most constrained meal first so broader slots do not take its options.
    let mut order: Vec<usize> = (0..options.len()).collect();
    order.sort_by_key(|&i| options[i].len());
    let mut best: Option<BudgetEstimate> = None;
    for seed in 0..24_usize {
        let mut selected: Vec<CostExample> = vec![];
        let mut last_used = BTreeMap::<String, u32>::new();
        let mut feasible = true;
        for day in 0..days {
            for &index in &order {
                let available: Vec<_> = options[index]
                    .iter()
                    .filter(|e| {
                        last_used
                            .get(&e.recipe_id)
                            .is_none_or(|last| day >= last + 7)
                    })
                    .collect();
                if available.is_empty() {
                    feasible = false;
                    break;
                }
                // Seed zero is the cheapest greedy rotation; others explore nearby
                // alternatives. All candidates preserve the seven-day cooldown.
                let rank = if seed == 0 {
                    0
                } else {
                    (seed + day as usize + index) % available.len().min(4)
                };
                let mut example = available[rank].clone();
                example.day = day;
                last_used.insert(example.recipe_id.clone(), day);
                selected.push(example);
            }
            if !feasible {
                break;
            }
        }
        if !feasible {
            continue;
        }
        selected.sort_by_key(|e| (e.day, e.meal));
        let groceries = cost_examples(c, &selected)?;
        let high_paise = groceries.iter().map(|g| g.high_paise).sum();
        if best.as_ref().is_none_or(|b| high_paise < b.high_paise) {
            best = Some(BudgetEstimate {
                low_paise: groceries.iter().map(|g| g.low_paise).sum(),
                high_paise,
                distinct_recipes: selected
                    .iter()
                    .map(|e| &e.recipe_id)
                    .collect::<BTreeSet<_>>()
                    .len(),
                examples: selected,
                groceries,
            });
        }
    }
    Ok(best)
}
/// Aggregate the actual selected portions across days, then buy whole packs once.
pub fn cost_examples(c: &Catalogue, examples: &[CostExample]) -> Result<Vec<Grocery>, String> {
    let mut totals = BTreeMap::<String, u64>::new();
    for example in examples {
        let recipe = c
            .recipes
            .iter()
            .find(|r| r.id == example.recipe_id)
            .ok_or("Unknown recipe")?;
        for amount in &recipe.ingredients {
            *totals.entry(amount.ingredient.clone()).or_default() +=
                (amount.quantity * u64::from(example.portion_percent)).div_ceil(100);
        }
    }
    groceries_from_totals(c, totals)
}

/// Cost of ingredient quantities consumed by one serving; excludes pack rounding.
pub fn recipe_portion_cost(c: &Catalogue, r: &Recipe) -> Result<(u64, u64), String> {
    recipe_allowed(
        c,
        r,
        &PlanRequest {
            diet: Diet::MeatAllowed,
            ..Default::default()
        },
    )?;
    let mut low = 0;
    let mut high = 0;
    for a in &r.ingredients {
        let i = c
            .ingredients
            .iter()
            .find(|i| i.id == a.ingredient)
            .ok_or("Unknown ingredient")?;
        low += (a.quantity * i.price.low_paise).div_ceil(i.price.pack_quantity);
        high += (a.quantity * i.price.high_paise).div_ceil(i.price.pack_quantity);
    }
    Ok((low, high))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn period_totals_and_fractional_targets_stay_consistent() {
        let mut p = FitnessProfile::default();
        assert_eq!(
            p.daily(),
            Nutrition {
                calories_kcal: 2100.0,
                protein_g: 120.0
            }
        );
        assert_eq!(p.daily().scaled(7.0).protein_g, 840.0);
        p.month_days = 31;
        assert_eq!(
            p.daily().scaled(f64::from(p.month_days)).calories_kcal,
            65100.0
        );
        p.targets[0].nutrition.protein_g = 30.5;
        assert_eq!(p.daily().scaled(30.0).protein_g, 3615.0);
    }
    #[test]
    fn estimates_meet_each_target_and_reconcile_full_packs() {
        let c = demo_catalogue();
        let p = FitnessProfile::default();
        let b = estimate_budget(&c, &p)
            .unwrap()
            .expect("default demo targets supported");
        assert_eq!(
            b.high_paise,
            b.groceries.iter().map(|g| g.high_paise).sum::<u64>()
        );
        assert_eq!(b.examples.len(), p.month_days as usize * p.targets.len());
        assert!(b.distinct_recipes >= 21);
        let mut last = BTreeMap::new();
        for e in &b.examples {
            if let Some(previous) = last.insert(&e.recipe_id, e.day) {
                assert!(e.day >= previous + 7);
            }
            let t = p.targets.iter().find(|t| t.meal == e.meal).unwrap();
            assert!(e.nutrition.protein_g >= t.nutrition.protein_g);
            assert!(
                (e.nutrition.calories_kcal - t.nutrition.calories_kcal).abs()
                    <= t.nutrition.calories_kcal * 0.1 + 0.001
            );
        }
        assert_eq!(b.groceries, cost_examples(&c, &b.examples).unwrap());
        for g in b.groceries {
            assert!(g.purchased >= g.needed);
            assert_eq!(g.high_paise, g.packs * g.price.high_paise);
        }
    }
    #[test]
    fn exclusions_and_unreachable_targets_do_not_produce_fake_minimums() {
        let c = demo_catalogue();
        let mut p = FitnessProfile {
            excluded_ingredients: c.ingredients.iter().map(|i| i.id.clone()).collect(),
            ..Default::default()
        };
        assert_eq!(estimate_budget(&c, &p).unwrap(), None);
        p.excluded_ingredients.clear();
        p.targets[0].nutrition = Nutrition {
            calories_kcal: 1000.0,
            protein_g: 250.0,
        };
        assert_eq!(estimate_budget(&c, &p).unwrap(), None);
        p.targets[0].nutrition.protein_g = f64::NAN;
        assert!(estimate_budget(&c, &p).is_err());
    }
    #[test]
    fn budget_and_priority_never_silently_reduce_nutrition() {
        let c = demo_catalogue();
        let mut p = FitnessProfile::default();
        let original = estimate_budget(&c, &p).unwrap();
        p.monthly_budget_paise = 1;
        p.diet_first = false;
        assert_eq!(estimate_budget(&c, &p).unwrap(), original);
    }
    #[test]
    fn a_single_valid_dish_is_never_repeated_to_fake_variety() {
        let mut c = demo_catalogue();
        c.recipes.retain(|r| r.id == "soy-bhurji-roti");
        let mut p = FitnessProfile::default();
        p.targets.truncate(1);
        assert_eq!(estimate_budget(&c, &p).unwrap(), None);
    }
    #[test]
    fn catalogue_has_valid_unique_foods_and_ingredient_derived_nutrition() {
        let c = demo_catalogue();
        assert_eq!(c.recipes.len(), 163);
        assert_eq!(
            c.recipes.len(),
            c.recipes
                .iter()
                .map(|r| &r.id)
                .collect::<BTreeSet<_>>()
                .len()
        );
        for r in &c.recipes {
            if r.id == "mix-rice" {
                continue;
            }
            let request = PlanRequest {
                diet: Diet::MeatAllowed,
                ..Default::default()
            };
            recipe_allowed(&c, r, &request).unwrap();
            let expected = r
                .ingredients
                .iter()
                .map(|a| {
                    c.ingredients
                        .iter()
                        .find(|i| i.id == a.ingredient)
                        .unwrap()
                        .nutrition_per_100_units
                        .unwrap()
                        .protein_g
                        * a.quantity as f64
                        / 100.0
                })
                .sum::<f64>();
            assert!((expected - r.nutrition.unwrap().protein_g).abs() <= 0.051);
        }
    }
    #[test]
    fn missing_nutrition_is_not_treated_as_zero() {
        let mut c = demo_catalogue();
        for r in &mut c.recipes {
            r.nutrition = None;
        }
        assert_eq!(
            estimate_budget(&c, &FitnessProfile::default()).unwrap(),
            None
        );
    }
}
