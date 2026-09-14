use serde::{Deserialize, Serialize};

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
