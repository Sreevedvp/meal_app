CREATE TABLE fitness_plans (
    id UUID PRIMARY KEY,
    catalogue_version TEXT NOT NULL REFERENCES catalogues(version),
    request JSONB NOT NULL,
    result JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX fitness_plans_created_idx ON fitness_plans(created_at DESC);
