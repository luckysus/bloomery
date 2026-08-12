use bloomery::domains::{
    cleanup_staging, compute_package_digest, install_package, load_package, official_trust_store,
    resolve_resource_path, sign_domain_package, DomainTrust, DomainTrustStore,
};
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::domains::{
    activate, impact, list, remove, upsert, DomainPackageImpact,
};
use ed25519_dalek::{Signer, SigningKey};
use rusqlite::Connection;
use serde_json::json;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zip::write::SimpleFileOptions;

struct TempPackage(PathBuf);

impl TempPackage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("bloomery-domain-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("assets")).expect("create package root");
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write_manifest(&self, value: serde_json::Value) {
        fs::write(
            self.path().join("manifest.json"),
            serde_json::to_vec_pretty(&value).expect("serialize manifest"),
        )
        .expect("write manifest");
    }
}

impl Drop for TempPackage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn valid_manifest() -> serde_json::Value {
    json!({
        "id": "steel",
        "version": "1.0.0",
        "compatibility": {"min_app_version": "0.1.0", "max_app_version": null},
        "author": "Bloomery contributors",
        "license": "Apache-2.0",
        "prompts": {"system": "Use steel terminology.", "workflow": "Cite the source."},
        "terminology": {"Q355B": "Chinese structural steel grade"},
        "retrieval": {"required_tags": ["steel"], "citation_required": true, "max_evidence_items": 12},
        "builtin_tool_allowlist": ["knowledge.query"],
        "mcp_recommendations": [{"id": "standards", "transport": "streamable_http", "description": "Standards lookup"}],
        "data_mappings": [{"dataset": "production", "fields": {"heat_id": "heat_id"}, "units": {"temperature": "C"}}],
        "evaluations": [{"id": "steel-qa", "kind": "qa", "dataset": "fixtures/qa.jsonl", "expected_behavior": "cite source", "threshold": 0.8}],
        "assets": [{"path": "assets/steel.json", "kind": "terminology", "sha256": null}]
    })
}

#[test]
fn loads_strict_manifest_and_resolves_declared_assets() {
    let package = TempPackage::new();
    fs::write(
        package.path().join("assets/steel.json"),
        "{\"grade\":\"Q355B\"}",
    )
    .expect("write asset");
    package.write_manifest(valid_manifest());

    let loaded = load_package(package.path(), "0.1.0").expect("load domain package");

    assert_eq!(loaded.manifest.id, "steel");
    assert_eq!(loaded.manifest.version, "1.0.0");
    assert_eq!(loaded.manifest.license, "Apache-2.0");
    assert_eq!(loaded.manifest.retrieval.max_evidence_items, 12);
    assert_eq!(
        loaded.assets[0].relative_path,
        PathBuf::from("assets/steel.json")
    );
}

#[test]
fn rejects_unknown_fields_and_executable_declarations() {
    let package = TempPackage::new();
    let mut manifest = valid_manifest();
    manifest["script"] = json!("install.ps1");
    package.write_manifest(manifest);

    let error = load_package(package.path(), "0.1.0").expect_err("script must be rejected");

    assert!(error.to_string().contains("unknown field") || error.to_string().contains("script"));
}

#[test]
fn rejects_incompatible_package_and_unsafe_assets() {
    let package = TempPackage::new();
    let mut manifest = valid_manifest();
    manifest["compatibility"]["min_app_version"] = json!("9.0.0");
    package.write_manifest(manifest);
    assert!(load_package(package.path(), "0.1.0")
        .expect_err("incompatible package must fail")
        .to_string()
        .contains("incompatible"));

    let root = package.path().to_path_buf();
    for path in [
        "../outside.json",
        "C:/outside.json",
        "assets/../outside.json",
    ] {
        assert!(
            resolve_resource_path(&root, path).is_err(),
            "unsafe path accepted: {path}"
        );
    }
}

#[test]
fn rejects_executable_asset_extensions() {
    let package = TempPackage::new();
    let mut manifest = valid_manifest();
    manifest["assets"][0]["path"] = json!("assets/run.ps1");
    package.write_manifest(manifest);
    fs::write(package.path().join("assets/run.ps1"), "Write-Host unsafe")
        .expect("write executable fixture");

    let error = load_package(package.path(), "0.1.0").expect_err("executable asset must fail");

    assert!(error.to_string().contains("executable"));
}

#[test]
fn rejects_asset_with_declared_hash_mismatch() {
    let package = TempPackage::new();
    fs::write(package.path().join("assets/steel.json"), "trusted content").expect("write asset");
    let mut manifest = valid_manifest();
    manifest["assets"][0]["sha256"] = json!("0".repeat(64));
    package.write_manifest(manifest);

    let error = load_package(package.path(), "0.1.0")
        .expect_err("declared asset hash mismatch must be rejected");

    assert!(error.to_string().contains("SHA-256"));
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_signature(package: &TempPackage, signing_key: &SigningKey, key_id: &str) {
    let digest = compute_package_digest(package.path()).expect("compute package digest");
    let signature = signing_key.sign(digest.as_bytes());
    fs::write(
        package.path().join("signature.json"),
        serde_json::to_vec_pretty(&json!({
            "key_id": key_id,
            "algorithm": "ed25519",
            "package_sha256": digest,
            "signature": hex(&signature.to_bytes())
        }))
        .expect("serialize signature"),
    )
    .expect("write signature");
}

#[test]
fn installs_signed_package_with_official_trust() {
    let package = TempPackage::new();
    fs::write(
        package.path().join("assets/steel.json"),
        "{\"grade\":\"Q355B\"}",
    )
    .expect("write asset");
    package.write_manifest(valid_manifest());
    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    write_signature(&package, &signing_key, "bloomery-official");
    let mut trust = DomainTrustStore::default();
    trust.add_official_key("bloomery-official", signing_key.verifying_key());
    let install_root = TempPackage::new();

    let installed = install_package(package.path(), install_root.path(), "0.1.0", &trust)
        .expect("install signed package");

    assert_eq!(installed.trust, DomainTrust::OfficialSigned);
    assert!(installed.path.is_dir());
    assert_eq!(installed.manifest.id, "steel");
}

#[test]
fn signs_a_domain_package_with_a_private_seed() {
    let package = TempPackage::new();
    fs::write(
        package.path().join("assets/steel.json"),
        "{\"grade\":\"Q355B\"}",
    )
    .expect("write asset");
    package.write_manifest(valid_manifest());

    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    sign_domain_package(package.path(), &signing_key, "bloomery-official-2026")
        .expect("sign domain package");

    let signature_path = package.path().join("signature.json");
    let envelope: serde_json::Value =
        serde_json::from_slice(&fs::read(&signature_path).expect("read signature"))
            .expect("decode signature");
    let digest = compute_package_digest(package.path()).expect("compute signed package digest");
    assert_eq!(envelope["key_id"], "bloomery-official-2026");
    assert_eq!(envelope["algorithm"], "ed25519");
    assert_eq!(envelope["package_sha256"], digest);
    assert_eq!(
        envelope["signature"].as_str().map(str::len),
        Some(128),
        "signature must contain 64 encoded bytes"
    );

    let mut trust = DomainTrustStore::default();
    trust.add_official_key("bloomery-official-2026", signing_key.verifying_key());
    let installed = install_package(package.path(), TempPackage::new().path(), "0.1.0", &trust)
        .expect("signed package must install as official");
    assert_eq!(installed.trust, DomainTrust::OfficialSigned);
}

#[test]
fn accepts_unsigned_package_as_explicit_third_party() {
    let package = TempPackage::new();
    fs::write(package.path().join("assets/steel.json"), "{}").expect("write asset");
    package.write_manifest(valid_manifest());
    let install_root = TempPackage::new();

    let installed = install_package(
        package.path(),
        install_root.path(),
        "0.1.0",
        &DomainTrustStore::default(),
    )
    .expect("install unsigned package");

    assert_eq!(installed.trust, DomainTrust::ThirdPartyUnsigned);
}

#[test]
fn rejects_zip_package_symlink_entries() {
    let source = TempPackage::new();
    let package_path = source.path().join("symlinked.zip");
    let file = fs::File::create(&package_path).expect("create domain ZIP");
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file("manifest.json", SimpleFileOptions::default())
        .expect("start manifest entry");
    archive
        .write_all(&serde_json::to_vec_pretty(&valid_manifest()).expect("serialize manifest"))
        .expect("write manifest entry");
    archive
        .add_symlink(
            "assets/steel.json",
            "../../outside.json",
            SimpleFileOptions::default(),
        )
        .expect("write symlink entry");
    archive.finish().expect("finish domain ZIP");

    let error = install_package(
        &package_path,
        TempPackage::new().path(),
        "0.1.0",
        &DomainTrustStore::default(),
    )
    .expect_err("ZIP symlink entry must be rejected");

    assert!(error.to_string().contains("symlink") || error.to_string().contains("non-regular"));
}

#[test]
fn rejects_signature_when_package_hash_changes() {
    let package = TempPackage::new();
    fs::write(package.path().join("assets/steel.json"), "original").expect("write asset");
    package.write_manifest(valid_manifest());
    let signing_key = SigningKey::from_bytes(&[8_u8; 32]);
    write_signature(&package, &signing_key, "official");
    fs::write(package.path().join("assets/steel.json"), "tampered").expect("tamper asset");
    let mut trust = DomainTrustStore::default();
    trust.add_official_key("official", signing_key.verifying_key());

    let error = install_package(package.path(), TempPackage::new().path(), "0.1.0", &trust)
        .expect_err("hash mismatch must fail");

    assert!(error.to_string().contains("hash"));
}

#[test]
fn rejects_signature_with_untrusted_key_id() {
    let package = TempPackage::new();
    fs::write(package.path().join("assets/steel.json"), "{}").expect("write asset");
    package.write_manifest(valid_manifest());
    let signing_key = SigningKey::from_bytes(&[9_u8; 32]);
    write_signature(&package, &signing_key, "unknown-key-id");
    let mut trust = DomainTrustStore::default();
    // Register the key under a different id so the envelope key_id does not resolve.
    trust.add_official_key("bloomery-official-2026", signing_key.verifying_key());

    let error = install_package(package.path(), TempPackage::new().path(), "0.1.0", &trust)
        .expect_err("untrusted key id must fail");

    assert!(error.to_string().contains("not trusted"));
}

#[test]
fn rejects_signature_signed_by_wrong_key() {
    let package = TempPackage::new();
    fs::write(package.path().join("assets/steel.json"), "{}").expect("write asset");
    package.write_manifest(valid_manifest());
    let signing_key = SigningKey::from_bytes(&[10_u8; 32]);
    write_signature(&package, &signing_key, "bloomery-official-2026");
    let mut trust = DomainTrustStore::default();
    // Same key_id, but a different key: verification must fail.
    let other_key = SigningKey::from_bytes(&[11_u8; 32]);
    trust.add_official_key("bloomery-official-2026", other_key.verifying_key());

    let error = install_package(package.path(), TempPackage::new().path(), "0.1.0", &trust)
        .expect_err("wrong signing key must fail");

    assert!(error.to_string().contains("verification failed"));
}

#[test]
fn official_trust_store_builds_with_embedded_key() {
    // The embedded official public key must be a valid Ed25519 key (no panic),
    // and an unsigned package must still install as third-party.
    let store = official_trust_store();
    let package = TempPackage::new();
    fs::write(package.path().join("assets/steel.json"), "{}").expect("write asset");
    package.write_manifest(valid_manifest());
    let install_root = TempPackage::new();

    let installed = install_package(package.path(), install_root.path(), "0.1.0", &store)
        .expect("install unsigned package against official trust store");

    assert_eq!(installed.trust, DomainTrust::ThirdPartyUnsigned);
}

#[test]
fn official_trust_store_rejects_the_known_throwaway_signing_key() {
    let package = TempPackage::new();
    fs::write(package.path().join("assets/steel.json"), "{}").expect("write asset");
    package.write_manifest(valid_manifest());

    // The previous implementation embedded the public key derived from this
    // publicly documented throwaway seed. It must never be a release trust
    // root, even if a package carries the expected key id.
    let throwaway_key = SigningKey::from_bytes(&[0x42_u8; 32]);
    write_signature(&package, &throwaway_key, "bloomery-official-2026");

    let error = install_package(
        package.path(),
        TempPackage::new().path(),
        "0.1.0",
        &official_trust_store(),
    )
    .expect_err("the public throwaway key must not authenticate an official package");

    assert!(
        error.to_string().contains("not trusted")
            || error.to_string().contains("verification failed"),
        "unexpected trust-store error: {error}"
    );
}

#[test]
fn keeps_previous_versions_for_activation_and_rollback() {
    let v1 = TempPackage::new();
    fs::write(v1.path().join("assets/steel.json"), "{}").expect("write first asset");
    v1.write_manifest(valid_manifest());
    let v2 = TempPackage::new();
    fs::write(v2.path().join("assets/steel.json"), "{}").expect("write second asset");
    let mut manifest = valid_manifest();
    manifest["version"] = json!("2.0.0");
    v2.write_manifest(manifest);
    let install_root = TempPackage::new();

    let first = install_package(
        v1.path(),
        install_root.path(),
        "0.1.0",
        &DomainTrustStore::default(),
    )
    .expect("install first version");
    let second = install_package(
        v2.path(),
        install_root.path(),
        "0.1.0",
        &DomainTrustStore::default(),
    )
    .expect("install second version");

    assert_ne!(first.path, second.path);
    assert!(first.path.is_dir());
    assert!(second.path.is_dir());
    assert_eq!(
        load_package(&first.path, "0.1.0").unwrap().manifest.version,
        "1.0.0"
    );
    assert_eq!(
        load_package(&second.path, "0.1.0")
            .unwrap()
            .manifest
            .version,
        "2.0.0"
    );
}

#[test]
fn cleans_only_domain_staging_directories_after_interrupted_install() {
    let root = TempPackage::new();
    let staging = root.path().join(".staging").join("interrupted");
    fs::create_dir_all(&staging).expect("create staging directory");
    fs::write(staging.join("partial"), "incomplete").expect("write partial package");

    let removed = cleanup_staging(root.path()).expect("cleanup staging");

    assert_eq!(removed, 1);
    assert!(!staging.exists());
}

#[test]
fn persists_domain_versions_with_one_active_version_per_package() {
    let first = TempPackage::new();
    fs::write(first.path().join("assets/steel.json"), "{}").expect("write first asset");
    first.write_manifest(valid_manifest());
    let second = TempPackage::new();
    fs::write(second.path().join("assets/steel.json"), "{}").expect("write second asset");
    let mut second_manifest = valid_manifest();
    second_manifest["version"] = json!("2.0.0");
    second.write_manifest(second_manifest);
    let install_root = TempPackage::new();
    let first = install_package(
        first.path(),
        install_root.path(),
        "0.1.0",
        &DomainTrustStore::default(),
    )
    .expect("install first version");
    let second = install_package(
        second.path(),
        install_root.path(),
        "0.1.0",
        &DomainTrustStore::default(),
    )
    .expect("install second version");
    let mut connection = Connection::open_in_memory().expect("open database");
    migrate(&mut connection).expect("migrate database");
    upsert(&mut connection, "local", &first).expect("persist first");
    upsert(&mut connection, "local", &second).expect("persist second");

    activate(&mut connection, "local", "steel", "1.0.0").expect("activate first");
    activate(&mut connection, "local", "steel", "2.0.0").expect("activate second");
    let records = list(&connection, "local").expect("list domain packages");

    assert_eq!(records.len(), 2);
    assert_eq!(records.iter().filter(|record| record.active).count(), 1);
    assert!(records
        .iter()
        .any(|record| record.version == "2.0.0" && record.active));
}

#[test]
fn active_package_cannot_be_removed_and_impact_preview_is_explicit() {
    let package = TempPackage::new();
    fs::write(package.path().join("assets/steel.json"), "{}").expect("write asset");
    package.write_manifest(valid_manifest());
    let install_root = TempPackage::new();
    let installed = install_package(
        package.path(),
        install_root.path(),
        "0.1.0",
        &DomainTrustStore::default(),
    )
    .expect("install package");
    let mut connection = Connection::open_in_memory().expect("open database");
    migrate(&mut connection).expect("migrate database");
    upsert(&mut connection, "local", &installed).expect("persist package");
    activate(&mut connection, "local", "steel", "1.0.0").expect("activate package");

    let preview = impact(&connection, "local", "steel", "1.0.0").expect("preview removal");
    assert_eq!(
        preview,
        DomainPackageImpact {
            package_id: "steel".to_string(),
            version: "1.0.0".to_string(),
            active: true,
            tool_count: 1,
            mcp_recommendation_count: 1,
            asset_count: 1,
        }
    );
    assert!(remove(&mut connection, "local", "steel", "1.0.0")
        .expect_err("active package must not be removed")
        .contains("active"));
}
