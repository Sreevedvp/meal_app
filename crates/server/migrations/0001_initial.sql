CREATE TABLE catalogues (
    version TEXT PRIMARY KEY,
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE plans (
    id UUID PRIMARY KEY,
    catalogue_version TEXT NOT NULL REFERENCES catalogues(version),
    request JSONB NOT NULL,
    result JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

