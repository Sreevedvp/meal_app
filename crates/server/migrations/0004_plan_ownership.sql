-- Existing prototype plans stay available only in loopback development mode.
-- Production identities are hashed and cannot equal this reserved owner.
ALTER TABLE plans ADD COLUMN owner_id TEXT NOT NULL DEFAULT 'development';
ALTER TABLE fitness_plans ADD COLUMN owner_id TEXT NOT NULL DEFAULT 'development';
ALTER TABLE fitness_plans ADD COLUMN revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0);
ALTER TABLE fitness_plans ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();
CREATE INDEX plans_owner_created_idx ON plans(owner_id, created_at DESC);
CREATE INDEX fitness_plans_owner_updated_idx ON fitness_plans(owner_id, updated_at DESC);
