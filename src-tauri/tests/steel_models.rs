use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::steel_models::{self as models, NewSteelModel};
use rusqlite::Connection;

const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SHA_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn migrated_connection() -> (std::path::PathBuf, Connection) {
    let path = std::env::temp_dir().join(format!(
        "bloomery-steel-models-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    let mut connection = Connection::open(&path).expect("open model database");
    migrate(&mut connection).expect("migrate model database");
    (path, connection)
}

fn linear_model(sha: &str) -> NewSteelModel<'_> {
    NewSteelModel {
        lineage_id: "linear:dataset-1",
        kind: "linear_artifact",
        source_task_id: Some("task-1"),
        model_sha256: sha,
        manifest_json: "{\"model_id\":\"m\"}",
        artifact_json: Some("{\"artifact_version\":\"linear-regression.v1\"}"),
        model_base64: None,
    }
}

#[test]
fn versions_increment_per_lineage_and_first_version_is_active() {
    let (path, mut connection) = migrated_connection();

    let first = models::create(&mut connection, "local", linear_model(SHA_A))
        .expect("create first version");
    assert_eq!(first.version, 1);
    assert!(first.is_active);

    let second = models::create(&mut connection, "local", linear_model(SHA_B))
        .expect("create second version");
    assert_eq!(second.version, 2);
    assert!(!second.is_active);

    let other_lineage = models::create(
        &mut connection,
        "local",
        NewSteelModel {
            lineage_id: "linear:dataset-2",
            ..linear_model(SHA_C)
        },
    )
    .expect("create other lineage version");
    assert_eq!(other_lineage.version, 1, "lineages version independently");
    assert!(other_lineage.is_active);

    drop(connection);
    let _ = std::fs::remove_file(path);
}

#[test]
fn set_active_switches_within_lineage_only() {
    let (path, mut connection) = migrated_connection();
    let first = models::create(&mut connection, "local", linear_model(SHA_A)).expect("v1");
    let second = models::create(&mut connection, "local", linear_model(SHA_B)).expect("v2");

    let activated = models::set_active(&mut connection, "local", &second.id).expect("activate v2");
    assert!(activated.is_active);

    let first_after = models::get(&connection, "local", &first.id)
        .expect("read v1")
        .expect("v1 exists");
    assert!(!first_after.is_active, "previous active must be cleared");

    drop(connection);
    let _ = std::fs::remove_file(path);
}

#[test]
fn active_model_versions_cannot_be_deleted() {
    let (path, mut connection) = migrated_connection();
    let first = models::create(&mut connection, "local", linear_model(SHA_A)).expect("v1");
    let second = models::create(&mut connection, "local", linear_model(SHA_B)).expect("v2");

    let error = models::delete(&mut connection, "local", &first.id)
        .expect_err("active version deletion must be rejected");
    assert_eq!(error, "active model versions cannot be deleted");

    models::delete(&mut connection, "local", &second.id).expect("inactive version deletes");
    assert!(models::get(&connection, "local", &second.id)
        .expect("read deleted")
        .is_none());

    drop(connection);
    let _ = std::fs::remove_file(path);
}

#[test]
fn list_orders_versions_descending() {
    let (path, mut connection) = migrated_connection();
    models::create(&mut connection, "local", linear_model(SHA_A)).expect("v1");
    models::create(&mut connection, "local", linear_model(SHA_B)).expect("v2");
    models::create(&mut connection, "local", linear_model(SHA_C)).expect("v3");

    let listed = models::list(&connection, "local", "linear:dataset-1").expect("list");
    assert_eq!(
        listed.iter().map(|model| model.version).collect::<Vec<_>>(),
        vec![3, 2, 1]
    );

    drop(connection);
    let _ = std::fs::remove_file(path);
}

#[test]
fn onnx_versions_require_a_blob_and_reject_artifacts() {
    let (path, mut connection) = migrated_connection();

    let error = models::create(
        &mut connection,
        "local",
        NewSteelModel {
            lineage_id: "onnx:task-1",
            kind: "onnx",
            source_task_id: None,
            model_sha256: SHA_A,
            manifest_json: "{}",
            artifact_json: Some("{}"),
            model_base64: None,
        },
    )
    .expect_err("onnx versions must store a blob");
    assert_eq!(
        error,
        "onnx model versions must store a blob and no artifact"
    );

    let record = models::create(
        &mut connection,
        "local",
        NewSteelModel {
            lineage_id: "onnx:task-1",
            kind: "onnx",
            source_task_id: None,
            model_sha256: SHA_A,
            manifest_json: "{\"model_id\":\"onnx-1\"}",
            artifact_json: None,
            model_base64: Some("AAAA"),
        },
    )
    .expect("onnx version with blob");
    assert_eq!(record.kind, "onnx");

    drop(connection);
    let _ = std::fs::remove_file(path);
}
