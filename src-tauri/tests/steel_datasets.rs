use bloomery::steel::{preview_dataset, DatasetPreviewRequest};
use bloomery::storage::{migrations::migrate, repositories::steel};
use rusqlite::Connection;
use std::fs;

fn database() -> Connection {
    let mut connection = Connection::open_in_memory().expect("open database");
    migrate(&mut connection).expect("migrate database");
    connection
}

#[test]
fn saves_a_preview_with_mapping_and_reuses_the_same_source_record() {
    let path = std::env::temp_dir().join(format!("bloomery-steel-{}.csv", uuid::Uuid::new_v4()));
    fs::write(
        &path,
        "heat_id,yield_strength,grade\nH-01,355,Q355B\nH-02,360,Q355B\n",
    )
    .expect("write fixture");
    let preview = preview_dataset(&DatasetPreviewRequest {
        source_path: path.to_string_lossy().into_owned(),
        sheet: None,
    })
    .expect("preview dataset");

    let mut connection = database();
    let mappings = vec![steel::DatasetColumnMapping {
        ordinal: 1,
        canonical_field: Some(" yield_strength ".to_string()),
        unit: Some(" MPa ".to_string()),
    }];
    let first = steel::save_preview(
        &mut connection,
        "workspace-a",
        &path.to_string_lossy(),
        "sha256-source",
        &preview,
        &mappings,
    )
    .expect("save dataset preview");

    assert_eq!(first.source_name, preview.source_name);
    assert_eq!(
        first.columns[1].canonical_field.as_deref(),
        Some("yield_strength")
    );
    assert_eq!(first.columns[1].unit.as_deref(), Some("MPa"));

    let invalid = steel::save_preview(
        &mut connection,
        "workspace-a",
        &path.to_string_lossy(),
        "sha256-invalid",
        &preview,
        &[steel::DatasetColumnMapping {
            ordinal: 1,
            canonical_field: Some("yield strength".to_string()),
            unit: Some("MPa".to_string()),
        }],
    )
    .expect_err("canonical field with spaces must be rejected");
    assert!(invalid.contains("canonical field"));

    let second = steel::save_preview(
        &mut connection,
        "workspace-a",
        &path.to_string_lossy(),
        "sha256-source",
        &preview,
        &mappings,
    )
    .expect("reuse dataset preview");
    assert_eq!(first.id, second.id);
    assert_eq!(
        steel::list(&connection, "workspace-a")
            .expect("list datasets")
            .len(),
        1
    );
    assert!(steel::list(&connection, "workspace-b")
        .expect("list other workspace datasets")
        .is_empty());

    let _ = fs::remove_file(path);
}
