use bloomery::steel::{preview_dataset, read_dataset_table, DatasetPreviewRequest};
use bloomery::storage::{migrations::migrate, repositories::steel};
use rusqlite::Connection;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

struct GeneratedXlsx(PathBuf);

impl GeneratedXlsx {
    fn create(name: &str, entries: &[(&str, &[u8])]) -> Self {
        let directory =
            std::env::temp_dir().join(format!("bloomery-steel-xlsx-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("create XLSX fixture directory");
        let path = directory.join(name);
        let file = fs::File::create(&path).expect("create XLSX fixture");
        let mut archive = zip::ZipWriter::new(file);
        for (entry_name, bytes) in entries {
            archive
                .start_file(*entry_name, SimpleFileOptions::default())
                .expect("start XLSX fixture entry");
            archive.write_all(bytes).expect("write XLSX fixture entry");
        }
        archive.finish().expect("finish XLSX fixture");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for GeneratedXlsx {
    fn drop(&mut self) {
        fs::remove_dir_all(self.0.parent().expect("XLSX fixture parent"))
            .expect("remove XLSX fixture directory");
    }
}

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
fn bounds_xlsx_rows_in_memory_without_losing_the_source_row_count() {
    let directory =
        std::env::temp_dir().join(format!("bloomery-steel-xlsx-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&directory).expect("create large XLSX fixture directory");
    let path = directory.join("large.xlsx");
    let file = fs::File::create(&path).expect("create large XLSX fixture");
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file("xl/workbook.xml", SimpleFileOptions::default())
        .expect("start workbook");
    archive
        .write_all(
            br#"<workbook xmlns:r="r"><sheets><sheet name="Heat Data" r:id="rId1"/></sheets></workbook>"#,
        )
        .expect("write workbook");
    archive
        .start_file("xl/_rels/workbook.xml.rels", SimpleFileOptions::default())
        .expect("start relationships");
    archive
        .write_all(
            br#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
        )
        .expect("write relationships");
    archive
        .start_file("xl/worksheets/sheet1.xml", SimpleFileOptions::default())
        .expect("start worksheet");
    archive
        .write_all(br#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>heat_id</t></is></c><c r="B1" t="inlineStr"><is><t>yield_strength</t></is></c></row>"#)
        .expect("write worksheet header");
    for row in 2..=100_002 {
        write!(
            archive,
            r#"<row r="{row}"><c r="A{row}" t="inlineStr"><is><t>H-{row}</t></is></c><c r="B{row}"><v>355</v></c></row>"#
        )
        .expect("write worksheet row");
    }
    archive
        .write_all(b"</sheetData></worksheet>")
        .expect("finish worksheet XML");
    archive.finish().expect("finish large XLSX fixture");

    let request = DatasetPreviewRequest {
        source_path: path.to_string_lossy().into_owned(),
        sheet: None,
    };
    let table = read_dataset_table(&request).expect("read bounded XLSX dataset table");
    let preview = preview_dataset(&request).expect("preview bounded XLSX dataset table");

    assert_eq!(table.rows.len(), 100_000);
    assert_eq!(preview.row_count, 100_001);
    assert!(preview.truncated);
    assert_eq!(preview.sample_rows.len(), 20);

    fs::remove_dir_all(directory).expect("remove large XLSX fixture directory");
}

#[test]
fn xlsx_dataset_parser_only_reads_the_requested_sheet() {
    let workbook = br#"<workbook xmlns:r="r"><sheets><sheet name="Broken" r:id="rId1"/><sheet name="Heat Data" r:id="rId2"/></sheets></workbook>"#;
    let relationships = br#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/sheet2.xml"/></Relationships>"#;
    let shared_strings =
        br#"<sst><si><t>grade</t></si><si><t>temperature</t></si><si><t>Q355B</t></si></sst>"#;
    let broken = br#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>999</v></c></row></sheetData></worksheet>"#;
    let heat_data = br#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row><row r="2"><c r="A2" t="s"><v>2</v></c><c r="B2"><f>1650-800</f><v>850</v></c></row></sheetData></worksheet>"#;
    let xlsx = GeneratedXlsx::create(
        "selected.xlsx",
        &[
            ("xl/workbook.xml", workbook),
            ("xl/_rels/workbook.xml.rels", relationships),
            ("xl/sharedStrings.xml", shared_strings),
            ("xl/worksheets/sheet1.xml", broken),
            ("xl/worksheets/sheet2.xml", heat_data),
        ],
    );

    let table = read_dataset_table(&DatasetPreviewRequest {
        source_path: xlsx.path().to_string_lossy().into_owned(),
        sheet: Some("Heat Data".to_string()),
    })
    .expect("read only the requested worksheet");

    assert_eq!(table.sheets, ["Broken", "Heat Data"]);
    assert_eq!(table.selected_sheet, "Heat Data");
    assert_eq!(table.headers, ["grade", "temperature"]);
    assert_eq!(table.rows, [["Q355B", "=1650-800"]]);
}

#[test]
fn xlsx_dataset_parser_preserves_logical_row_gaps() {
    let workbook = br#"<workbook xmlns:r="r"><sheets><sheet name="Heat Data" r:id="rId1"/></sheets></workbook>"#;
    let relationships = br#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#;
    let worksheet = br#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>grade</t></is></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>Q355B</t></is></c></row></sheetData></worksheet>"#;
    let xlsx = GeneratedXlsx::create(
        "sparse.xlsx",
        &[
            ("xl/workbook.xml", workbook),
            ("xl/_rels/workbook.xml.rels", relationships),
            ("xl/worksheets/sheet1.xml", worksheet),
        ],
    );

    let table = read_dataset_table(&DatasetPreviewRequest {
        source_path: xlsx.path().to_string_lossy().into_owned(),
        sheet: None,
    })
    .expect("read sparse worksheet");

    assert_eq!(table.row_count, 2);
    assert_eq!(table.rows, [[""], ["Q355B"]]);
}

#[test]
fn xlsx_dataset_parser_skips_empty_sheets_when_no_sheet_is_requested() {
    let workbook = br#"<workbook xmlns:r="r"><sheets><sheet name="Cover" r:id="rId1"/><sheet name="Heat Data" r:id="rId2"/></sheets></workbook>"#;
    let relationships = br#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/sheet2.xml"/></Relationships>"#;
    let cover = br#"<worksheet><sheetData></sheetData></worksheet>"#;
    let heat_data = br#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>grade</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>Q355B</t></is></c></row></sheetData></worksheet>"#;
    let xlsx = GeneratedXlsx::create(
        "with-cover.xlsx",
        &[
            ("xl/workbook.xml", workbook),
            ("xl/_rels/workbook.xml.rels", relationships),
            ("xl/worksheets/sheet1.xml", cover),
            ("xl/worksheets/sheet2.xml", heat_data),
        ],
    );

    let table = read_dataset_table(&DatasetPreviewRequest {
        source_path: xlsx.path().to_string_lossy().into_owned(),
        sheet: None,
    })
    .expect("select first non-empty worksheet by default");

    assert_eq!(table.sheets, ["Heat Data"]);
    assert_eq!(table.selected_sheet, "Heat Data");
    assert_eq!(table.rows, [["Q355B"]]);
}

#[test]
fn xlsx_dataset_parser_does_not_decode_values_after_the_preview_limit() {
    let directory =
        std::env::temp_dir().join(format!("bloomery-steel-xlsx-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&directory).expect("create capped XLSX fixture directory");
    let path = directory.join("capped.xlsx");
    let file = fs::File::create(&path).expect("create capped XLSX fixture");
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file("xl/workbook.xml", SimpleFileOptions::default())
        .expect("start workbook");
    archive
        .write_all(
            br#"<workbook xmlns:r="r"><sheets><sheet name="Heat Data" r:id="rId1"/></sheets></workbook>"#,
        )
        .expect("write workbook");
    archive
        .start_file("xl/_rels/workbook.xml.rels", SimpleFileOptions::default())
        .expect("start relationships");
    archive
        .write_all(
            br#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
        )
        .expect("write relationships");
    archive
        .start_file("xl/sharedStrings.xml", SimpleFileOptions::default())
        .expect("start shared strings");
    archive
        .write_all(br#"<sst><si><t>heat_id</t></si><si><t>H-1</t></si></sst>"#)
        .expect("write shared strings");
    archive
        .start_file("xl/worksheets/sheet1.xml", SimpleFileOptions::default())
        .expect("start worksheet");
    archive
        .write_all(br#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row>"#)
        .expect("write worksheet header");
    for row in 2..=100_001 {
        write!(
            archive,
            r#"<row r="{row}"><c r="A{row}" t="s"><v>1</v></c></row>"#
        )
        .expect("write preview row");
    }
    archive
        .write_all(
            br#"<row r="100002"><c r="A100002" t="s"><v>999</v></c></row></sheetData></worksheet>"#,
        )
        .expect("write capped bad row");
    archive.finish().expect("finish capped XLSX fixture");

    let table = read_dataset_table(&DatasetPreviewRequest {
        source_path: path.to_string_lossy().into_owned(),
        sheet: None,
    })
    .expect("ignore bad shared string outside preview limit");

    assert_eq!(table.rows.len(), 100_000);
    assert_eq!(table.row_count, 100_001);
    assert!(table.truncated);

    fs::remove_dir_all(directory).expect("remove capped XLSX fixture directory");
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
