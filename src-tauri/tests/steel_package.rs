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
        .assets
        .iter()
        .all(|asset| !asset.path.extension().is_some_and(|extension| {
            matches!(extension.to_str(), Some("exe" | "dll" | "ps1" | "cmd"))
        })));
}
