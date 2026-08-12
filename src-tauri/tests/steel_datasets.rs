use bloomery::steel::{preview_dataset, read_dataset_table, DatasetPreviewRequest};
use bloomery::storage::{migrations::migrate, repositories::steel};
use rusqlite::Connection;
use std::fs;
use std::io::{BufWriter, Write};

fn database() -> Connection {
    let mut connection = Connection::open_in_memory().expect("open database");
    migrate(&mut connection).expect("migrate database");
    connection
}

#[test]
fn bounds_csv_rows_in_memory_without_losing_the_source_row_count() {
    let path =
        std::env::temp_dir().join(format!("bloomery-steel-large-{}.csv", uuid::Uuid::new_v4()));
    let file = fs::File::create(&path).expect("create large CSV fixture");
    let mut writer = BufWriter::new(file);
    writeln!(writer, "heat_id,yield_strength").expect("write header");
    for index in 0..100_001 {
        writeln!(writer, "H-{index},355").expect("write row");
    }
    writer.flush().expect("flush large CSV fixture");
    let request = DatasetPreviewRequest {
        source_path: path.to_string_lossy().into_owned(),
        sheet: None,
    };

    let table = read_dataset_table(&request).expect("read bounded dataset table");
    let preview = preview_dataset(&request).expect("preview bounded dataset table");

    assert_eq!(table.rows.len(), 100_000);
    assert_eq!(preview.row_count, 100_001);
    assert!(preview.truncated);
    assert_eq!(preview.sample_rows.len(), 20);

    let _ = fs::remove_file(path);
}

#[test]
fn csv_dataset_parser_preserves_unicode_and_escaped_quotes() {
    let path = std::env::temp_dir().join(format!(
        "bloomery-steel-unicode-{}.csv",
        uuid::Uuid::new_v4()
    ));
    fs::write(
        &path,
        "\u{feff}heat_id,notes\nH-01,\"Q355B热轧, \"\"稳定\"\"\"\n",
    )
    .expect("write fixture");

    let table = read_dataset_table(&DatasetPreviewRequest {
        source_path: path.to_string_lossy().into_owned(),
        sheet: None,
    })
    .expect("read CSV dataset table");

    assert_eq!(table.headers[0], "heat_id");
    assert_eq!(table.rows[0][1], "Q355B热轧, \"稳定\"");

    let _ = fs::remove_file(path);
}

#[test]
fn csv_dataset_parser_rejects_unknown_sheet_and_warns_on_ragged_rows() {
    let path = std::env::temp_dir().join(format!(
        "bloomery-steel-ragged-{}.csv",
        uuid::Uuid::new_v4()
    ));
    fs::write(&path, "heat_id,yield_strength\nH-01,355,extra\n").expect("write fixture");

    let unknown_sheet = read_dataset_table(&DatasetPreviewRequest {
        source_path: path.to_string_lossy().into_owned(),
        sheet: Some("Sheet1".to_string()),
    })
    .expect_err("CSV only exposes the CSV sheet");
    assert!(unknown_sheet.contains("sheet"));

    let preview = preview_dataset(&DatasetPreviewRequest {
        source_path: path.to_string_lossy().into_owned(),
        sheet: Some("CSV".to_string()),
    })
    .expect("preview ragged CSV dataset");

    assert!(preview
        .warnings
        .iter()
        .any(|warning| warning.contains("inconsistent column counts")));

    let _ = fs::remove_file(path);
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

#[test]
fn activates_only_a_dataset_with_a_canonical_mapping() {
    let path = std::env::temp_dir().join(format!("bloomery-steel-{}.csv", uuid::Uuid::new_v4()));
    fs::write(&path, "heat_id,yield_strength\nH-01,355\n").expect("write fixture");
    let preview = preview_dataset(&DatasetPreviewRequest {
        source_path: path.to_string_lossy().into_owned(),
        sheet: None,
    })
    .expect("preview dataset");
    let mut connection = database();

    let draft = steel::save_preview(
        &mut connection,
        "workspace-a",
        &path.to_string_lossy(),
        "sha256-ready",
        &preview,
        &[steel::DatasetColumnMapping {
            ordinal: 1,
            canonical_field: Some("yield_strength".to_string()),
            unit: Some("MPa".to_string()),
        }],
    )
    .expect("save mapped dataset");
    let ready = steel::activate(&mut connection, "workspace-a", &draft.id)
        .expect("activate mapped dataset")
        .expect("activated dataset");
    assert_eq!(ready.mapping_state, "ready");

    let unmapped = steel::save_preview(
        &mut connection,
        "workspace-a",
        &path.to_string_lossy(),
        "sha256-draft",
        &preview,
        &[],
    )
    .expect("save unmapped dataset");
    let error = steel::activate(&mut connection, "workspace-a", &unmapped.id)
        .expect_err("unmapped dataset must not activate");
    assert!(error.contains("canonical field"));

    let _ = fs::remove_file(path);
}

#[cfg(windows)]
#[test]
fn rejects_dataset_symlink_that_resolves_outside_its_selected_directory() {
    use std::os::windows::fs::symlink_file;

    let root = std::env::temp_dir().join(format!("bloomery-steel-path-{}", uuid::Uuid::new_v4()));
    let selected = root.join("selected");
    let outside = root.join("outside");
    fs::create_dir_all(&selected).expect("create selected directory");
    fs::create_dir_all(&outside).expect("create outside directory");
    let source = outside.join("source.csv");
    let linked = selected.join("source.csv");
    fs::write(&source, "heat_id,yield_strength\nH-01,355\n").expect("write outside fixture");
    if symlink_file(&source, &linked).is_err() {
        let _ = fs::remove_dir_all(root);
        return;
    }

    let result = preview_dataset(&DatasetPreviewRequest {
        source_path: linked.to_string_lossy().into_owned(),
        sheet: None,
    });

    assert!(
        result.is_err(),
        "dataset symlink must not escape its selected directory"
    );
    let _ = fs::remove_dir_all(root);
}
