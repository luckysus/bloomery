use bloomery::rag::ingest::SourceFormat;
use bloomery::rag::model::SourceLocation;
use bloomery::rag::parse::{parse_document, DocumentBlock, ParseLimits};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zip::write::SimpleFileOptions;

struct GeneratedFixture(PathBuf);

impl GeneratedFixture {
    fn file(name: &str, bytes: &[u8]) -> Self {
        let directory = std::env::temp_dir().join(format!("bloomery-rag-parse-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("create generated fixture directory");
        let path = directory.join(name);
        fs::write(&path, bytes).expect("write generated fixture");
        Self(path)
    }

    fn zip(name: &str, entries: &[(&str, &[u8])]) -> Self {
        let directory = std::env::temp_dir().join(format!("bloomery-rag-parse-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("create generated fixture directory");
        let path = directory.join(name);
        let file = fs::File::create(&path).expect("create ZIP fixture");
        let mut archive = zip::ZipWriter::new(file);
        for (entry_name, bytes) in entries {
            archive
                .start_file(*entry_name, SimpleFileOptions::default())
                .expect("start ZIP fixture entry");
            archive.write_all(bytes).expect("write ZIP fixture entry");
        }
        archive.finish().expect("finish ZIP fixture");
        Self(path)
    }

    fn symlink_zip(name: &str, entry_name: &str, target: &str) -> Self {
        let directory = std::env::temp_dir().join(format!("bloomery-rag-parse-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("create generated fixture directory");
        let path = directory.join(name);
        let file = fs::File::create(&path).expect("create ZIP fixture");
        let mut archive = zip::ZipWriter::new(file);
        archive
            .add_symlink(entry_name, target, SimpleFileOptions::default())
            .expect("add ZIP symlink fixture");
        archive.finish().expect("finish ZIP fixture");
        Self(path)
    }

    fn duplicate_zip(name: &str) -> Self {
        let fixture = Self::zip(
            name,
            &[
                ("word/document-a.xml", b"<w:document/>"),
                ("word/document-b.xml", b"<w:document><w:body/></w:document>"),
            ],
        );
        let mut bytes = fs::read(fixture.path()).expect("read duplicate ZIP fixture");
        let from = b"word/document-b.xml";
        let to = b"word/document-a.xml";
        for index in 0..=bytes.len() - from.len() {
            if &bytes[index..index + from.len()] == from {
                bytes[index..index + from.len()].copy_from_slice(to);
            }
        }
        fs::write(fixture.path(), bytes).expect("rewrite duplicate ZIP fixture names");
        fixture
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for GeneratedFixture {
    fn drop(&mut self) {
        fs::remove_dir_all(self.0.parent().expect("fixture parent"))
            .expect("remove generated fixture directory");
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/documents")
        .join(name)
}

#[test]
fn markdown_snapshot_preserves_structure_formula_and_remote_image_warning() {
    let parsed = parse_document(
        &fixture("steel.md"),
        SourceFormat::Markdown,
        ParseLimits::default(),
    )
    .expect("parse Markdown");

    assert_eq!(
        serde_json::to_value(&parsed.blocks).expect("serialize blocks"),
        serde_json::json!([
            {"kind":"heading","level":1,"text":"Q355B 工艺","location":{"kind":"text_offsets","start":0,"end":14}},
            {"kind":"paragraph","text":"Q355B 连铸过程记录。","location":{"kind":"text_offsets","start":16,"end":43}},
            {"kind":"list","ordered":false,"items":["炉温 1500 C","终点碳 0.08%"],"location":{"kind":"text_offsets","start":45,"end":78}},
            {"kind":"formula","text":"C_{eq}=C+Mn/6","location":{"kind":"text_offsets","start":80,"end":97}},
            {"kind":"table","rows":[["炉次","温度"],["H-001","1500"]],"location":{"kind":"text_offsets","start":99,"end":150}},
            {"kind":"image","alt":"金相图","asset_index":null,"location":{"kind":"text_offsets","start":152,"end":204}}
        ])
    );
    assert!(parsed.assets.is_empty());
    assert_eq!(parsed.warnings.len(), 1);
    assert_eq!(parsed.warnings[0].code, "remote_asset_ignored");
}

#[test]
fn text_parser_preserves_paragraph_offsets_and_unicode() {
    let parsed = parse_document(
        &fixture("steel.txt"),
        SourceFormat::Text,
        ParseLimits::default(),
    )
    .expect("parse text");

    assert_eq!(parsed.blocks.len(), 2);
    assert!(matches!(
        &parsed.blocks[0],
        DocumentBlock::Paragraph { text, .. } if text == "Q355B production note"
    ));
    assert!(matches!(
        &parsed.blocks[1],
        DocumentBlock::Paragraph { text, .. }
            if text.contains("355 MPa") && text.contains("Latin identifiers")
    ));
    assert!(matches!(
        parsed.blocks[1].location(),
        SourceLocation::TextOffsets { start, end } if start < end
    ));
}

#[test]
fn html_parser_keeps_structure_and_never_loads_remote_images() {
    let parsed = parse_document(
        &fixture("steel.html"),
        SourceFormat::Html,
        ParseLimits::default(),
    )
    .expect("parse HTML");

    assert!(parsed
        .blocks
        .iter()
        .any(|block| matches!(block, DocumentBlock::Heading { text, .. } if text == "高炉记录")));
    assert!(parsed.blocks.iter().any(
        |block| matches!(block, DocumentBlock::Paragraph { text, .. } if text == "炉温 1500 C。")
    ));
    assert!(parsed.blocks.iter().any(|block| matches!(
        block,
        DocumentBlock::List { items, .. } if items == &["Q355B", "Q420B"]
    )));
    assert!(parsed.blocks.iter().any(|block| matches!(
        block,
        DocumentBlock::Table { rows, .. }
            if rows == &[vec!["炉次".to_string(), "温度".to_string()], vec!["H-001".to_string(), "1500".to_string()]]
    )));
    assert!(parsed.assets.is_empty());
    assert_eq!(parsed.warnings[0].code, "remote_asset_ignored");
}

#[test]
fn csv_parser_handles_quotes_and_reports_sheet_ranges() {
    let parsed = parse_document(
        &fixture("steel.csv"),
        SourceFormat::Csv,
        ParseLimits::default(),
    )
    .expect("parse CSV");

    assert_eq!(parsed.blocks.len(), 1);
    assert!(matches!(
        &parsed.blocks[0],
        DocumentBlock::Table { rows, location }
            if rows[2][0] == "H,002"
                && matches!(location, SourceLocation::SheetRange { sheet, range } if sheet == "CSV" && range == "A1:C3")
    ));
}

#[test]
fn parser_rejects_files_over_its_read_limit() {
    let error = parse_document(
        &fixture("steel.txt"),
        SourceFormat::Text,
        ParseLimits {
            max_source_bytes: 8,
            ..ParseLimits::default()
        },
    )
    .unwrap_err();

    assert_eq!(error.code(), "parse_source_too_large");
}

#[test]
fn local_pdf_parser_returns_page_locations_and_an_explicit_quality_warning() {
    let pdf = GeneratedFixture::file(
        "standard.pdf",
        br#"%PDF-1.4
1 0 obj << /Type /Page /Contents 2 0 R >> endobj
2 0 obj << /Length 72 >> stream
BT /F1 12 Tf 72 720 Td (Q355B yield strength 355 MPa) Tj ET
endstream endobj
%%EOF"#,
    );

    let parsed = parse_document(pdf.path(), SourceFormat::Pdf, ParseLimits::default())
        .expect("parse local PDF text layer");

    assert!(matches!(
        &parsed.blocks[0],
        DocumentBlock::Paragraph { text, location }
            if text == "Q355B yield strength 355 MPa"
                && matches!(location, SourceLocation::PdfPage { page: 1, bbox: None })
    ));
    assert!(parsed
        .warnings
        .iter()
        .any(|warning| warning.code == "pdf_text_layer_limited"));
}

#[test]
fn docx_parser_preserves_headings_lists_tables_formulas_and_images() {
    let document = br#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="w" xmlns:m="m"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Rolling process</w:t></w:r></w:p>
<w:p><w:r><w:t>Q355B finishing temperature 860 C</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr/></w:pPr><w:r><w:t>Heat billet</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr/></w:pPr><w:r><w:t>Finish roll</w:t></w:r></w:p>
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Grade</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Temp</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>Q355B</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>860</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
<m:oMath><m:r><m:t>C_eq=C+Mn/6</m:t></m:r></m:oMath>
<w:p><w:r><w:drawing/></w:r></w:p>
</w:body></w:document>"#;
    let docx = GeneratedFixture::zip(
        "rolling.docx",
        &[
            ("word/document.xml", document),
            ("word/media/micrograph.png", b"\x89PNG\r\n\x1a\nfixture"),
        ],
    );

    let parsed = parse_document(docx.path(), SourceFormat::Docx, ParseLimits::default())
        .expect("parse DOCX");

    assert!(matches!(
        &parsed.blocks[0],
        DocumentBlock::Heading { level: 1, text, .. } if text == "Rolling process"
    ));
    assert!(parsed.blocks.iter().any(|block| matches!(
        block,
        DocumentBlock::List { items, .. } if items == &["Heat billet", "Finish roll"]
    )));
    assert!(parsed.blocks.iter().any(|block| matches!(
        block,
        DocumentBlock::Table { rows, .. } if rows[1] == ["Q355B", "860"]
    )));
    assert!(parsed.blocks.iter().any(|block| matches!(
        block,
        DocumentBlock::Formula { text, .. } if text == "C_eq=C+Mn/6"
    )));
    assert_eq!(parsed.assets.len(), 1);
    assert_eq!(parsed.assets[0].media_type, "image/png");
    assert!(parsed.blocks.iter().any(|block| matches!(
        block,
        DocumentBlock::Image {
            asset_index: Some(0),
            ..
        }
    )));
}

#[test]
fn xlsx_parser_resolves_shared_strings_formulas_and_sheet_ranges() {
    let workbook = br#"<workbook xmlns:r="r"><sheets><sheet name="Heat Data" r:id="rId1"/></sheets></workbook>"#;
    let relationships = br#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#;
    let shared_strings =
        br#"<sst><si><t>Grade</t></si><si><t>Q355B</t></si><si><t>Temperature</t></si></sst>"#;
    let worksheet = br#"<worksheet><sheetData>
<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>2</v></c></row>
<row r="2"><c r="A2" t="s"><v>1</v></c><c r="B2"><v>1650</v></c><c r="C2"><f>B2-800</f><v>850</v></c></row>
</sheetData></worksheet>"#;
    let xlsx = GeneratedFixture::zip(
        "heats.xlsx",
        &[
            ("xl/workbook.xml", workbook),
            ("xl/_rels/workbook.xml.rels", relationships),
            ("xl/sharedStrings.xml", shared_strings),
            ("xl/worksheets/sheet1.xml", worksheet),
        ],
    );

    let parsed = parse_document(xlsx.path(), SourceFormat::Xlsx, ParseLimits::default())
        .expect("parse XLSX");

    assert!(parsed.blocks.iter().any(|block| matches!(
        block,
        DocumentBlock::Table { rows, location }
            if rows[1] == ["Q355B", "1650", "=B2-800"]
                && matches!(location, SourceLocation::SheetRange { sheet, range } if sheet == "Heat Data" && range == "A1:C2")
    )));
    assert!(parsed.blocks.iter().any(|block| matches!(
        block,
        DocumentBlock::Formula { text, location }
            if text == "B2-800"
                && matches!(location, SourceLocation::SheetRange { sheet, range } if sheet == "Heat Data" && range == "C2")
    )));
}

#[test]
fn office_archives_reject_traversal_and_expansion_limits() {
    let traversal = GeneratedFixture::zip(
        "unsafe.docx",
        &[
            ("word/document.xml", b"<w:document/>"),
            ("../escape.xml", b"escape"),
        ],
    );
    assert_eq!(
        parse_document(traversal.path(), SourceFormat::Docx, ParseLimits::default(),)
            .unwrap_err()
            .code(),
        "archive_path_traversal"
    );

    let expanded = GeneratedFixture::zip(
        "large.xlsx",
        &[("xl/workbook.xml", b"<workbook>0123456789</workbook>")],
    );
    assert_eq!(
        parse_document(
            expanded.path(),
            SourceFormat::Xlsx,
            ParseLimits {
                max_expanded_bytes: 8,
                ..ParseLimits::default()
            },
        )
        .unwrap_err()
        .code(),
        "archive_expanded_too_large"
    );
}

#[test]
fn office_archives_reject_duplicate_and_symlink_entries() {
    let duplicate = GeneratedFixture::duplicate_zip("duplicate.docx");
    assert_eq!(
        parse_document(duplicate.path(), SourceFormat::Docx, ParseLimits::default(),)
            .unwrap_err()
            .code(),
        "archive_duplicate_entry"
    );

    let symlink =
        GeneratedFixture::symlink_zip("symlink.xlsx", "xl/workbook.xml", "../../outside.xml");
    assert_eq!(
        parse_document(symlink.path(), SourceFormat::Xlsx, ParseLimits::default(),)
            .unwrap_err()
            .code(),
        "archive_symlink"
    );
}

#[test]
fn markdown_ignores_non_http_image_schemes_at_the_parse_boundary() {
    let markdown = GeneratedFixture::file(
        "schemes.md",
        br#"# Scheme boundary

![xss](javascript:alert(1))

![inline](data:text/plain;base64,SGVsbG8=)

![local](file:///etc/passwd)
"#,
    );

    let parsed = parse_document(
        markdown.path(),
        SourceFormat::Markdown,
        ParseLimits::default(),
    )
    .expect("parse Markdown");

    // 没有任何资产被加载/内嵌。
    assert!(parsed.assets.is_empty());
    let images: Vec<_> = parsed
        .blocks
        .iter()
        .filter(|block| matches!(block, DocumentBlock::Image { .. }))
        .collect();
    assert_eq!(images.len(), 3);
    for block in &images {
        assert!(
            matches!(
                block,
                DocumentBlock::Image {
                    asset_index: None,
                    ..
                }
            ),
            "non-http image must never be embedded"
        );
    }
    // 三个非 http/https scheme 均被标记为未内嵌（忽略于解析边界），而非 remote_asset_ignored。
    let scheme_warnings = parsed
        .warnings
        .iter()
        .filter(|warning| warning.code == "external_asset_not_embedded")
        .count();
    assert_eq!(scheme_warnings, 3);
    assert!(parsed
        .warnings
        .iter()
        .all(|warning| warning.code != "remote_asset_ignored"));
    // 危险 scheme 的 URL 不得残留在解析产物中。
    let serialized = serde_json::to_string(&parsed.blocks).expect("serialize blocks");
    assert!(!serialized.contains("javascript:"));
    assert!(!serialized.contains("data:"));
    assert!(!serialized.contains("file://"));
}

#[test]
fn html_ignores_non_http_image_schemes_at_the_parse_boundary() {
    let html = GeneratedFixture::file(
        "schemes.html",
        br#"<html><body>
<h1>Scheme boundary</h1>
<img alt="xss" src="javascript:alert(1)">
<img alt="inline" src="data:text/plain;base64,SGVsbG8=">
<img alt="local" src="file:///etc/passwd">
</body></html>"#,
    );

    let parsed = parse_document(html.path(), SourceFormat::Html, ParseLimits::default())
        .expect("parse HTML");

    assert!(parsed.assets.is_empty());
    let images: Vec<_> = parsed
        .blocks
        .iter()
        .filter(|block| matches!(block, DocumentBlock::Image { .. }))
        .collect();
    assert_eq!(images.len(), 3);
    for block in &images {
        assert!(matches!(
            block,
            DocumentBlock::Image {
                asset_index: None,
                ..
            }
        ));
    }
    let scheme_warnings = parsed
        .warnings
        .iter()
        .filter(|warning| warning.code == "external_asset_not_embedded")
        .count();
    assert_eq!(scheme_warnings, 3);
    assert!(parsed
        .warnings
        .iter()
        .all(|warning| warning.code != "remote_asset_ignored"));
    let serialized = serde_json::to_string(&parsed.blocks).expect("serialize blocks");
    assert!(!serialized.contains("javascript:"));
    assert!(!serialized.contains("file://"));
}
