-- Searchable, versioned records; retain catalogue snapshots for historical plans.
CREATE TABLE food_ingredients (
    catalogue_version TEXT NOT NULL REFERENCES catalogues(version),
    id TEXT NOT NULL,
    name TEXT NOT NULL,
    data JSONB NOT NULL,
    PRIMARY KEY (catalogue_version, id)
);
CREATE TABLE food_recipes (
    catalogue_version TEXT NOT NULL REFERENCES catalogues(version),
    id TEXT NOT NULL,
    name TEXT NOT NULL,
    cuisine TEXT NOT NULL,
    category TEXT NOT NULL,
    vegetarian BOOLEAN NOT NULL,
    meal_types TEXT[] NOT NULL,
    data JSONB NOT NULL,
    search_document TSVECTOR GENERATED ALWAYS AS
        (to_tsvector('simple', name || ' ' || cuisine || ' ' || category)) STORED,
    PRIMARY KEY (catalogue_version, id)
);
CREATE TABLE food_recipe_ingredients (
    catalogue_version TEXT NOT NULL,
    recipe_id TEXT NOT NULL,
    ingredient_id TEXT NOT NULL,
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    PRIMARY KEY (catalogue_version, recipe_id, ingredient_id),
    FOREIGN KEY (catalogue_version, recipe_id) REFERENCES food_recipes(catalogue_version, id),
    FOREIGN KEY (catalogue_version, ingredient_id) REFERENCES food_ingredients(catalogue_version, id)
);
CREATE INDEX food_recipes_search_idx ON food_recipes USING GIN(search_document);
CREATE INDEX food_recipes_meals_idx ON food_recipes USING GIN(meal_types);
CREATE INDEX food_recipes_browse_idx ON food_recipes(catalogue_version, vegetarian, name, id);
CREATE INDEX food_ingredient_reverse_idx ON food_recipe_ingredients(catalogue_version, ingredient_id);
