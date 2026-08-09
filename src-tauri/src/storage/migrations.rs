use super::StorageError;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

pub struct Migration {
    pub version: u32,
    pub sql: &'static str,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct MigrationReport {
    pub applied_versions: Vec<u32>,
    pub legacy_cloud_jobs: i64,
    pub legacy_cloud_settings: i64,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("migrations/0001_initial.sql"),
    },
    Migration {
        version: 2,
        sql: include_str!("migrations/0002_local_workspace.sql"),
    },
    Migration {
        version: 3,
        sql: include_str!("migrations/0003_provider_profiles.sql"),
    },
    Migration {
        version: 4,
        sql: include_str!("migrations/0004_background_tasks.sql"),
    },
    Migration {
        version: 5,
        sql: include_str!("migrations/0005_knowledge.sql"),
    },
    Migration {
        version: 6,
        sql: include_str!("migrations/0006_embedding_vectors.sql"),
    },
    Migration {
        version: 7,
        sql: include_str!("migrations/0007_pending_document_manifest.sql"),
    },
    Migration {
        version: 8,
        sql: include_str!("migrations/0008_provider_profile_revisions.sql"),
    },
    Migration {
        version: 9,
        sql: include_str!("migrations/0009_knowledge_fts.sql"),
    },
    Migration {
        version: 10,
        sql: include_str!("migrations/0010_retrieval_audits.sql"),
    },
    Migration {
        version: 11,
        sql: include_str!("migrations/0011_agent_runs.sql"),
    },
    Migration {
        version: 12,
        sql: include_str!("migrations/0012_agent_memory.sql"),
    },
    Migration {
        version: 13,
        sql: include_str!("migrations/0013_backfill_summary_source.sql"),
    },
    Migration {
        version: 14,
        sql: include_str!("migrations/0014_permission_rules.sql"),
    },
    Migration {
        version: 15,
        sql: include_str!("migrations/0015_domain_packages.sql"),
    },
    Migration {
        version: 16,
        sql: include_str!("migrations/0016_steel_datasets.sql"),
    },
    Migration {
        version: 17,
        sql: include_str!("migrations/0017_mcp_servers.sql"),
    },
    Migration {
        version: 18,
        sql: include_str!("migrations/0018_mcp_legacy_sse.sql"),
    },
];

pub fn latest_version() -> u32 {
    MIGRATIONS.last().map_or(0, |migration| migration.version)
}

pub fn migrate(connection: &mut Connection) -> Result<MigrationReport, StorageError> {
    let current = read_user_version(connection)?;
    let latest = latest_version();
    if current > latest {
        return Err(StorageError::new(
            "database_too_new",
            format!("database version {current} is newer than supported version {latest}"),
        ));
    }

    let mut report = MigrationReport::default();
    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.version > current)
    {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| migration_error(migration.version, error))?;
        let (cloud_jobs, cloud_settings) = if migration.version == 2 {
            legacy_cloud_counts(&transaction)?
        } else {
            (0, 0)
        };
        apply_migration(&transaction, migration, cloud_jobs, cloud_settings)?;
        transaction
            .commit()
            .map_err(|error| migration_error(migration.version, error))?;
        report.applied_versions.push(migration.version);
        if migration.version == 2 {
            report.legacy_cloud_jobs = cloud_jobs;
            report.legacy_cloud_settings = cloud_settings;
        }
    }

    Ok(report)
}

fn apply_migration(
    transaction: &Transaction<'_>,
    migration: &Migration,
    cloud_jobs: i64,
    cloud_settings: i64,
) -> Result<(), StorageError> {
    transaction
        .execute_batch(migration.sql)
        .map_err(|error| migration_error(migration.version, error))?;
    transaction
        .execute(
            "INSERT OR REPLACE INTO schema_migrations (version, applied_at)
             VALUES (?1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            params![migration.version],
        )
        .map_err(|error| migration_error(migration.version, error))?;
    if migration.version == 2 {
        transaction
            .execute(
                "INSERT OR REPLACE INTO migration_reports
                   (version, legacy_cloud_jobs, legacy_cloud_settings, migrated_at)
                 VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                params![migration.version, cloud_jobs, cloud_settings],
            )
            .map_err(|error| migration_error(migration.version, error))?;
    }
    transaction
        .pragma_update(None, "user_version", migration.version)
        .map_err(|error| migration_error(migration.version, error))?;
    Ok(())
}

fn legacy_cloud_counts(transaction: &Transaction<'_>) -> Result<(i64, i64), StorageError> {
    let cloud_jobs = table_count(transaction, "cloud_jobs")?;
    let cloud_settings = if table_exists(transaction, "settings")? {
        transaction
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE key = 'cloud_api_base'",
                [],
                |row| row.get(0),
            )
            .map_err(|error| migration_error(2, error))?
    } else {
        0
    };
    Ok((cloud_jobs, cloud_settings))
}

fn table_count(transaction: &Transaction<'_>, table: &str) -> Result<i64, StorageError> {
    if !table_exists(transaction, table)? {
        return Ok(0);
    }
    transaction
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .map_err(|error| migration_error(2, error))
}

fn table_exists(transaction: &Transaction<'_>, table: &str) -> Result<bool, StorageError> {
    transaction
        .query_row(
            "SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1",
            params![table],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(|error| migration_error(2, error))
}

fn read_user_version(connection: &Connection) -> Result<u32, StorageError> {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| StorageError::new("database_version_failed", error.to_string()))
}

fn migration_error(version: u32, error: impl std::fmt::Display) -> StorageError {
    StorageError::new(
        "migration_failed",
        format!("migration {version} failed: {error}"),
    )
}
