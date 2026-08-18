use bloomery::domains::load_package;
use std::path::PathBuf;

fn steel_package_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../domain-packs/steel")
}
#[test]
fn official_steel_package_fixture_is_declaration_only_and_loadable() {
    let package = load_package(&steel_package_root(), env!("CARGO_PKG_VERSION"))
        .expect("official steel package fixture must load");

    assert_eq!(package.manifest.id, "steel");
    assert_eq!(package.manifest.version, "1.0.0");
    assert_eq!(package.manifest.license, "Apache-2.0");
    assert!(package.manifest.prompts.system.contains("steel"));
    assert!(package
        .manifest
        .builtin_tool_allowlist
        .contains(&"knowledge.query".to_string()));
    assert!(package
        .manifest
        .builtin_tool_allowlist
        .contains(&"steel.carbon_equivalent".to_string()));
    for tool_id in [
        "steel.search_literature",
        "steel.read_literature_section",
        "steel.query_production_data",
        "steel.query_composition_standard",
        "steel.query_process_standard",
        "steel.ask_llm_with_context",
        "steel.get_model_status",
        "steel.predict_performance",
        "steel.optimize_process",
        "steel.match_coil",
        "steel.start_training",
        "steel.process_literature",
        "steel.export_data",
        "steel.remember_memory",
        "steel.read_memory",
        "steel.search_memory",
        "steel.list_memory",
        "steel.forget_memory",
    ] {
        assert!(
            package
                .manifest
                .builtin_tool_allowlist
                .contains(&tool_id.to_string()),
            "official steel package must allow {tool_id}"
        );
    }
    assert!(package
        .assets
        .iter()
        .all(|asset| !asset.path.extension().is_some_and(|extension| {
            matches!(extension.to_str(), Some("exe" | "dll" | "ps1" | "cmd"))
        })));
}
