ALTER TABLE provider_profiles
  ADD COLUMN revision INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1);

ALTER TABLE provider_profiles
  ADD COLUMN secret_generation INTEGER NOT NULL DEFAULT 0 CHECK (secret_generation >= 0);
