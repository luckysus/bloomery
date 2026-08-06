CREATE TABLE IF NOT EXISTS domain_packages (
  workspace_id TEXT NOT NULL,
  id TEXT NOT NULL,
  version TEXT NOT NULL,
  path TEXT NOT NULL,
  package_sha256 TEXT NOT NULL,
  trust TEXT NOT NULL CHECK (trust IN ('official_signed', 'third_party_unsigned')),
  manifest_json TEXT NOT NULL,
  installed_at TEXT NOT NULL,
  active INTEGER NOT NULL DEFAULT 0 CHECK (active IN (0, 1)),
  PRIMARY KEY (workspace_id, id, version)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_domain_packages_active
  ON domain_packages(workspace_id, id)
  WHERE active = 1;

CREATE INDEX IF NOT EXISTS idx_domain_packages_list
  ON domain_packages(workspace_id, id, version);
