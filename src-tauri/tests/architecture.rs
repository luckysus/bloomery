use std::fs;
use std::path::{Path, PathBuf};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn source(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path.as_ref())
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.as_ref().display()))
}

fn line_count(path: impl AsRef<Path>) -> usize {
    source(path).lines().count()
}

#[test]
fn tauri_entrypoint_has_no_auth_or_cloud_commands() {
    let main = source(manifest_dir().join("src/main.rs"));
    for forbidden in [
        "mod auth;",
        "mod cloud_tasks;",
        "auth_get_session",
        "auth_save_session",
        "auth_clear_session",
        "desktop_cloud_task_request",
        "sync_cloud_jobs",
    ] {
        assert!(
            !main.contains(forbidden),
            "src/main.rs contains forbidden command or module: {forbidden}"
        );
    }
}

#[test]
fn transitional_rust_files_stay_within_budget() {
    let root = manifest_dir().join("src");
    let budgets = [("db.rs", 400)];

    for (name, budget) in budgets {
        let actual = line_count(root.join(name));
        assert!(
            actual <= budget,
            "src/{name} has {actual} lines; transitional budget is {budget}"
        );
    }
}

#[test]
fn task_11_removes_legacy_runtime_and_enforces_module_budgets() {
    let source_root = manifest_dir().join("src");
    assert!(
        !source_root.join("local_agent.rs").exists(),
        "the legacy local_agent.rs runtime must be removed"
    );

    let runtime_root = source_root.join("agent/runtime");
    for entry in fs::read_dir(&runtime_root).expect("read agent runtime modules") {
        let path = entry.expect("agent runtime entry").path();
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        assert!(
            line_count(&path) <= 500,
            "{} exceeds the 500-line runtime module budget",
            path.display()
        );
    }

    let repository_root = source_root.join("storage/repositories");
    let mut pending = vec![repository_root];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).expect("read repository module directory") {
            let path = entry.expect("repository module entry").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|value| value.to_str()) != Some("rs") {
                continue;
            }
            assert!(
                line_count(&path) <= 400,
                "{} exceeds the 400-line repository module budget",
                path.display()
            );
        }
    }

    let mut app_modules = vec![source_root.join("app")];
    while let Some(directory) = app_modules.pop() {
        for entry in fs::read_dir(&directory).expect("read app command modules") {
            let path = entry.expect("app module entry").path();
            if path.is_dir() {
                app_modules.push(path);
                continue;
            }
            if path.extension().and_then(|value| value.to_str()) != Some("rs") {
                continue;
            }
            let module = source(&path);
            if !module.contains("#[tauri::command]") {
                continue;
            }
            let production = module.split("#[cfg(test)]").next().unwrap_or(&module);
            assert!(
                production.lines().count() <= 150,
                "{} exceeds the 150-line Tauri command module budget",
                path.display()
            );
        }
    }
}

#[test]
fn local_storage_and_context_do_not_require_auth_state() {
    for name in ["db.rs", "context.rs"] {
        let module = source(manifest_dir().join("src").join(name));
        for forbidden in ["AuthState", "current_user_id"] {
            assert!(
                !module.contains(forbidden),
                "src/{name} still depends on {forbidden}"
            );
        }
    }
}

#[test]
fn rust_runtime_has_no_auth_or_private_cloud_paths() {
    let checks = [
        ("lib.rs", vec!["mod auth;", "mod cloud_tasks;"]),
        (
            "app/mod.rs",
            vec![
                "auth::",
                "cloud_tasks::",
                "desktop_confirm_cloud_job",
                "list_cloud_jobs",
            ],
        ),
        (
            "agent/desktop/model.rs",
            vec![
                "AuthState",
                "ConfirmedCloudTask",
                "fetch_cloud_knowledge",
                "CLOUD_API_BASE_KEY",
            ],
        ),
        (
            "agent/desktop/provider.rs",
            vec![
                "AuthState",
                "ConfirmedCloudTask",
                "fetch_cloud_knowledge",
                "CLOUD_API_BASE_KEY",
            ],
        ),
        (
            "db.rs",
            vec!["CloudJob", "list_cloud_jobs", "upsert_cloud_job_for_user"],
        ),
        ("models.rs", vec!["struct CloudJob", "struct CloudJobInput"]),
    ];

    for (name, forbidden_values) in checks {
        let module = source(manifest_dir().join("src").join(name));
        for forbidden in forbidden_values {
            assert!(
                !module.contains(forbidden),
                "src/{name} still contains private runtime path {forbidden}"
            );
        }
    }
}

#[test]
fn independent_app_composition_module_exists() {
    let app = manifest_dir().join("src/app/mod.rs");
    assert!(
        app.is_file(),
        "src/app/mod.rs must own Tauri application composition"
    );
}

#[test]
fn command_registration_has_a_single_module() {
    let root = manifest_dir().join("src/app");
    assert!(
        root.join("commands.rs").is_file(),
        "src/app/commands.rs must own Tauri command registration"
    );
    let app = source(root.join("mod.rs"));
    assert!(
        app.contains("commands::handler!()"),
        "app composition must use commands::handler!()"
    );
    assert!(!app.contains("tauri::generate_handler!"));
}
#[test]
fn tauri_frontend_hooks_run_from_the_frontend_directory() {
    let config = source(manifest_dir().join("tauri.conf.json"));

    assert!(
        config.contains(r#""beforeDevCommand": "npm --prefix frontend run dev -- --host 127.0.0.1""#),
        "beforeDevCommand must address the frontend package from Tauri's repository working directory"
    );
    assert!(
        config.contains(r#""beforeBuildCommand": "npm --prefix frontend run build""#),
        "beforeBuildCommand must address the frontend package from Tauri's repository working directory"
    );
}
#[test]
fn database_runtime_uses_ordered_workspace_migrations() {
    let source_root = manifest_dir().join("src");
    let database = source(source_root.join("db.rs"));
    assert!(
        !source_root.join("schema.sql").exists(),
        "the legacy monolithic schema must be removed after migrations take ownership"
    );
    for migration in [
        "0001_initial.sql",
        "0002_local_workspace.sql",
        "0003_provider_profiles.sql",
        "0004_background_tasks.sql",
        "0005_knowledge.sql",
        "0006_embedding_vectors.sql",
        "0007_pending_document_manifest.sql",
        "0008_provider_profile_revisions.sql",
        "0009_knowledge_fts.sql",
    ] {
        assert!(
            source_root
                .join("storage/migrations")
                .join(migration)
                .is_file(),
            "missing ordered migration {migration}"
        );
    }
    assert!(
        database.contains("crate::storage::database::open"),
        "db_init must open SQLite through the ordered migration boundary"
    );
    assert!(
        !database.contains("execute_batch(include_str!(\"schema.sql\"))"),
        "runtime and test setup must not bypass ordered migrations"
    );

    for name in ["db.rs", "context.rs"] {
        let module = source(manifest_dir().join("src").join(name));
        assert!(
            !module.contains("user_id")
                && !module.contains(r#"execute_batch(include_str!("schema.sql"))"#),
            "src/{name} still uses the legacy user_id storage scope"
        );
    }
}

#[test]
fn repositories_and_task_storage_are_tauri_independent() {
    let repository_root = manifest_dir().join("src/storage/repositories");
    for entry in fs::read_dir(&repository_root).expect("read repository modules") {
        let path = entry.expect("repository entry").path();
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        let module = source(&path);
        assert!(
            !module.contains("tauri::") && !module.contains("#[tauri::command]"),
            "{} must not depend on Tauri",
            path.display()
        );
    }

    for name in ["tasks/model.rs", "tasks/repository.rs"] {
        let path = manifest_dir().join("src").join(name);
        let module = source(&path);
        assert!(
            !module.contains("tauri::") && !module.contains("#[tauri::command]"),
            "{} must not depend on Tauri",
            path.display()
        );
    }

    let commands = source(manifest_dir().join("src/app/storage_commands.rs"));
    for sql in ["SELECT ", "INSERT ", "UPDATE ", "DELETE "] {
        assert!(
            !commands.contains(sql),
            "storage command adapters must delegate SQL instead of containing {sql}"
        );
    }
}

#[test]
fn agent_session_is_a_sql_free_tauri_independent_domain_boundary() {
    let root = manifest_dir().join("src/agent/session");
    for entry in fs::read_dir(&root).expect("read agent session modules") {
        let path = entry.expect("agent session entry").path();
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        let module = source(&path);
        assert!(
            module.lines().count() <= 400,
            "{} exceeds the 400-line session module budget",
            path.display()
        );
        for forbidden in [
            "tauri::",
            "#[tauri::command]",
            "SELECT ",
            "INSERT ",
            "UPDATE ",
            "DELETE ",
        ] {
            assert!(
                !module.contains(forbidden),
                "{} must not contain {forbidden}",
                path.display()
            );
        }
    }

    let service = source(root.join("service.rs"));
    assert!(
        service.contains("TransactionBehavior::Immediate")
            && service.contains("runs::create_in_transaction"),
        "SessionService must own atomic user-message and run creation"
    );

    let commands = source(manifest_dir().join("src/app/storage_commands.rs"));
    assert!(
        commands.contains("SessionService"),
        "storage commands must construct the shared session service"
    );
    assert!(
        !commands.contains("conversations::"),
        "conversation commands must delegate through SessionService"
    );
}

#[test]
fn agent_context_is_bounded_and_sql_free_tauri_independent() {
    let root = manifest_dir().join("src/agent/context");
    for entry in fs::read_dir(&root).expect("read agent context modules") {
        let path = entry.expect("agent context entry").path();
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        let module = source(&path);
        assert!(
            module.lines().count() <= 400,
            "{} exceeds the 400-line context module budget",
            path.display()
        );
        for forbidden in [
            "tauri::",
            "#[tauri::command]",
            "SELECT ",
            "INSERT ",
            "UPDATE ",
            "DELETE ",
            "HashMap",
        ] {
            assert!(
                !module.contains(forbidden),
                "{} must not contain {forbidden}",
                path.display()
            );
        }
    }
}

#[test]
fn memory_repository_modules_are_bounded_and_user_lifecycle_commands_are_registered() {
    let root = manifest_dir().join("src/storage/repositories");
    let mut files = vec![root.join("memories.rs")];
    let memory_root = root.join("memories");
    if memory_root.is_dir() {
        files.extend(
            fs::read_dir(memory_root)
                .expect("read memory repository modules")
                .map(|entry| entry.expect("memory repository entry").path()),
        );
    }
    for path in files {
        let module = source(&path);
        assert!(
            module.lines().count() <= 400,
            "{} exceeds the 400-line memory repository budget",
            path.display()
        );
        assert!(
            !module.contains("tauri::") && !module.contains("#[tauri::command]"),
            "{} must remain Tauri-independent",
            path.display()
        );
    }

    let registrations = source(manifest_dir().join("src/app/commands.rs"));
    for command in [
        "confirm_memory_candidate",
        "reject_memory_candidate",
        "set_memory_enabled",
        "delete_memory",
    ] {
        assert!(
            registrations.contains(command),
            "memory lifecycle command {command} must be registered"
        );
    }

    let legacy_context = source(manifest_dir().join("src/context.rs"));
    assert!(
        legacy_context.contains("status = 'confirmed'"),
        "pending or rejected memories must not enter the active context"
    );
}

#[test]
fn registered_local_agent_routes_session_state_through_session_service() {
    let registrations = source(manifest_dir().join("src/app/commands.rs"));
    assert!(
        registrations.contains("crate::app::desktop_chat_commands::desktop_agent_chat"),
        "active desktop agent chat command must remain registered"
    );

    let agent = source(manifest_dir().join("src/agent/desktop/session.rs"));
    assert!(
        agent.contains("SessionService::new") && agent.contains(".start_run("),
        "active local agent must construct SessionService and start runs through it"
    );
    for forbidden in [
        "conversations::ensure(",
        "conversations::append_message(",
        "conversations::list_messages(",
        "conversations::latest_summary(",
        "conversations::save_summary(",
    ] {
        assert!(
            !agent.contains(forbidden),
            "active local agent bypasses SessionService with {forbidden}"
        );
    }
}

#[test]
fn knowledge_repository_modules_are_bounded_and_tauri_independent() {
    let root = manifest_dir().join("src/storage/repositories");
    let mut files = vec![root.join("knowledge.rs")];
    files.extend(
        fs::read_dir(root.join("knowledge"))
            .expect("read knowledge repository modules")
            .map(|entry| entry.expect("knowledge module").path()),
    );
    for path in files {
        let module = source(&path);
        assert!(
            module.lines().count() <= 400,
            "{} exceeds the 400-line repository budget",
            path.display()
        );
        assert!(
            !module.contains("tauri::") && !module.contains("#[tauri::command]"),
            "{} must remain Tauri-independent",
            path.display()
        );
    }
}

#[test]
fn rag_domain_modules_are_bounded_and_runtime_independent() {
    let mut directories = vec![manifest_dir().join("src/rag")];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory).expect("read RAG module directory") {
            let path = entry.expect("RAG module entry").path();
            if path.is_dir() {
                directories.push(path);
                continue;
            }
            if path.extension().and_then(|value| value.to_str()) != Some("rs") {
                continue;
            }
            let module = source(&path);
            assert!(
                module.lines().count() <= 400,
                "{} exceeds the 400-line RAG module budget",
                path.display()
            );
            for forbidden in ["tauri::", "#[tauri::command]", "reqwest::"] {
                assert!(
                    !module.contains(forbidden),
                    "{} must not contain {forbidden}",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn scheduler_is_tauri_independent_and_registered_after_migration() {
    let source_root = manifest_dir().join("src");
    let scheduler = source(source_root.join("tasks/scheduler.rs"));
    assert!(
        !scheduler.contains("tauri::") && !scheduler.contains("#[tauri::command]"),
        "scheduler core must remain testable without Tauri"
    );
    let noop_sink = scheduler
        .find("pub struct NoopEventSink")
        .expect("test event sink must remain available");
    assert!(
        scheduler[noop_sink.saturating_sub(40)..noop_sink].contains("#[cfg(test)]"),
        "NoopEventSink must not be compiled into production"
    );

    let database = source(source_root.join("db.rs"));
    let migration = database
        .find("crate::storage::database::open")
        .expect("db_init must run ordered migrations");
    let scheduler_start = database
        .find(".start(scheduler)")
        .expect("db_init must start the durable scheduler");
    assert!(
        migration < scheduler_start,
        "scheduler must start only after database migration"
    );
    assert!(
        database.contains("TauriEventSink::new") && !database.contains("NoopEventSink"),
        "production scheduler must emit progress through the Tauri event sink"
    );

    let event_sink = source(source_root.join("app/event_sink.rs"));
    assert!(
        event_sink.contains("app.emit(\"scheduler:progress\"")
            && event_sink.contains("impl EventSink for TauriEventSink"),
        "Tauri event sink must publish scheduler progress"
    );

    let app = source(source_root.join("app/mod.rs"));
    for required in [
        ".manage(SchedulerState::default())",
        ".build(tauri::generate_context!())",
        "RunEvent::ExitRequested { api, .. }",
        "shutdown(Duration::from_secs(2))",
        "api.prevent_exit()",
        "RunEvent::Exit",
        "request_shutdown()",
    ] {
        assert!(
            app.contains(required),
            "app lifecycle is missing scheduler integration: {required}"
        );
    }
}

#[test]
fn secret_commands_never_return_secret_values() {
    let registrations = source(manifest_dir().join("src/app/commands.rs"));
    for command in ["secret_set", "secret_status", "secret_delete"] {
        assert!(
            registrations.contains(command),
            "missing safe secret command {command}"
        );
    }
    assert!(
        !registrations.contains("secret_get"),
        "Tauri must never register a command that returns secret values"
    );

    let commands = source(manifest_dir().join("src/app/secret_commands.rs"));
    let commands = commands
        .split("#[cfg(test)]")
        .next()
        .expect("secret command production source");
    assert!(!commands.contains("Result<SecretValue"));
    assert!(!commands.contains("get_password"));
}

#[test]
fn local_document_import_is_registered_through_a_sql_free_adapter() {
    let source_root = manifest_dir().join("src");
    let registrations = source(source_root.join("app/commands.rs"));
    assert!(
        registrations.contains("knowledge_commands::commands::import_local_document"),
        "missing local document import command"
    );
    let adapter = source(source_root.join("app/knowledge_commands.rs"));
    assert!(adapter.contains("queue_document_import"));
    for forbidden in ["SELECT ", "INSERT ", "UPDATE ", "DELETE FROM"] {
        assert!(
            !adapter.contains(forbidden),
            "knowledge command adapter contains SQL: {forbidden}"
        );
    }
}

#[test]
fn http_clients_are_built_only_by_the_shared_factory() {
    let source_root = manifest_dir().join("src");
    let shared_http = source_root.join("providers/http.rs");
    assert!(shared_http.is_file(), "missing shared provider HTTP module");

    let mut pending = vec![source_root];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).expect("read Rust source directory") {
            let path = entry.expect("Rust source entry").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|value| value.to_str()) != Some("rs")
                || path == shared_http
            {
                continue;
            }
            let module = source(&path);
            for forbidden in [
                "reqwest::Client::new",
                "reqwest::Client::builder",
                "Client::builder",
            ] {
                assert!(
                    !module.contains(forbidden),
                    "{} bypasses the shared HTTP client factory with {forbidden}",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn desktop_agent_routes_chat_through_provider_capabilities() {
    let agent = source(manifest_dir().join("src/agent/desktop/provider.rs"));

    assert!(
        agent.contains("configured_chat_provider(profile, credential)"),
        "local agent must delegate concrete provider selection"
    );
    assert!(
        agent.contains("require(ProviderCapability::Chat)"),
        "local agent must check normalized chat capabilities"
    );
    assert!(
        !agent.contains("if profile.kind == ProviderKind::Ollama"),
        "local agent must not branch on provider brands at execution time"
    );
}
