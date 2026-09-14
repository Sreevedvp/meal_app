use meal_typing::*;
use serde::{Deserialize, Serialize};

// --- Budget plan types ---

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

// --- Fitness types ---

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

// --- Planner / calendar types ---

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
    pub request: PlannerRequest,
    pub meals: Vec<CostExample>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StoredFitnessPlan {
    pub plan: FitnessPlan,
    pub catalogue: Catalogue,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FitnessSwap {
    pub recipe_id: String,
    pub portion_percent: u32,
    pub nutrition: Nutrition,
    pub high_paise: u64,
}
