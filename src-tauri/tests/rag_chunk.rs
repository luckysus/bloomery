use bloomery::rag::chunk::{chunk_document, ChunkPolicy};
use bloomery::rag::model::SourceLocation;
use bloomery::rag::parse::{DocumentBlock, ParsedDocument};

fn location(start: u64, end: u64) -> SourceLocation {
    SourceLocation::TextOffsets { start, end }
}

fn document(blocks: Vec<DocumentBlock>) -> ParsedDocument {
    ParsedDocument {
        blocks,
        assets: Vec::new(),
        warnings: Vec::new(),
    }
}

fn policy() -> ChunkPolicy {
    ChunkPolicy {
        version: "steel-v1".to_string(),
        target_tokens: 8,
        max_tokens: 10,
        overlap_tokens: 2,
        table_header_rows: 1,
    }
}

#[test]
fn chunks_cjk_and_latin_paragraphs_with_heading_context_and_overlap() {
    let parsed = document(vec![
        DocumentBlock::Heading {
            level: 1,
            text: "Blast furnace 高炉".to_string(),
            location: location(0, 20),
        },
        DocumentBlock::Paragraph {
            text: "Q355B strength improves when 高炉 温度 pressure carbon oxygen stay stable"
                .to_string(),
            location: location(21, 100),
        },
    ]);

    let chunks = chunk_document(&parsed, &policy()).expect("chunk document");

    assert!(chunks.len() > 1);
    assert!(chunks
        .iter()
        .all(|chunk| chunk.text.starts_with("# Blast furnace 高炉\n\n")));
    assert!(chunks.iter().all(|chunk| chunk.token_count <= 10));
    assert_eq!(chunks[0].source_location, location(21, 100));
    assert!(chunks[0].text.contains("Q355B strength"));
    assert!(chunks[0].text.contains("高炉"));
    assert!(chunks[1].text.contains("高炉"));
}

#[test]
fn chunk_ids_and_text_are_deterministic_and_policy_versioned() {
    let parsed = document(vec![DocumentBlock::Paragraph {
        text: "Q235 Q355 Q420 steel grades".to_string(),
        location: SourceLocation::PdfPage {
            page: 3,
            bbox: None,
        },
    }]);
    let first = chunk_document(&parsed, &policy()).unwrap();
    let second = chunk_document(&parsed, &policy()).unwrap();
    assert_eq!(first, second);
    assert!(first[0].id.as_str().starts_with("chunk-"));
    assert_eq!(first[0].content_sha256.len(), 64);

    let mut changed = policy();
    changed.version = "steel-v2".to_string();
    assert_ne!(
        first[0].id,
        chunk_document(&parsed, &changed).unwrap()[0].id
    );
}

#[test]
fn formulas_and_image_captions_remain_explicit_chunks() {
    let parsed = document(vec![
        DocumentBlock::Formula {
            text: "C + O2 -> CO2".to_string(),
            location: location(0, 13),
        },
        DocumentBlock::Image {
            alt: "SEM inclusion morphology".to_string(),
            asset_index: Some(0),
            location: SourceLocation::PdfPage {
                page: 8,
                bbox: None,
            },
        },
    ]);

    let chunks = chunk_document(&parsed, &policy()).unwrap();

    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].text, "$$C + O2 -> CO2$$");
    assert_eq!(chunks[1].text, "[Image: SEM inclusion morphology]");
}

#[test]
fn long_formula_slices_keep_formula_boundaries() {
    let parsed = document(vec![DocumentBlock::Formula {
        text: "Fe C O Mn Si P S".to_string(),
        location: location(0, 16),
    }]);
    let mut formula_policy = policy();
    formula_policy.target_tokens = 4;
    formula_policy.max_tokens = 4;
    formula_policy.overlap_tokens = 1;

    let chunks = chunk_document(&parsed, &formula_policy).unwrap();

    assert!(chunks.len() > 1);
    assert!(chunks
        .iter()
        .all(|chunk| chunk.text.starts_with("$$") && chunk.text.ends_with("$$")));
    assert!(chunks.iter().all(|chunk| chunk.token_count <= 4));
}

#[test]
fn oversized_tables_repeat_headers_in_bounded_row_windows() {
    let parsed = document(vec![DocumentBlock::Table {
        rows: vec![
            vec!["Grade".into(), "Yield".into()],
            vec!["Q235".into(), "235 MPa".into()],
            vec!["Q355".into(), "355 MPa".into()],
            vec!["Q420".into(), "420 MPa".into()],
            vec!["Q460".into(), "460 MPa".into()],
        ],
        location: SourceLocation::SheetRange {
            sheet: "Grades".to_string(),
            range: "A1:B5".to_string(),
        },
    }]);
    let mut table_policy = policy();
    table_policy.target_tokens = 6;
    table_policy.max_tokens = 8;
    table_policy.overlap_tokens = 0;

    let chunks = chunk_document(&parsed, &table_policy).unwrap();

    assert!(chunks.len() >= 2);
    assert!(chunks
        .iter()
        .all(|chunk| chunk.text.starts_with("| Grade | Yield |")));
    assert!(chunks.iter().all(|chunk| chunk.token_count <= 8));
    let joined = chunks
        .iter()
        .map(|chunk| chunk.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for grade in ["Q235", "Q355", "Q420", "Q460"] {
        assert_eq!(joined.matches(grade).count(), 1);
    }
}

#[test]
fn chunk_policy_rejects_unbounded_or_non_progressing_configuration() {
    for invalid in [
        ChunkPolicy {
            target_tokens: 0,
            ..policy()
        },
        ChunkPolicy {
            target_tokens: 11,
            ..policy()
        },
        ChunkPolicy {
            overlap_tokens: 8,
            ..policy()
        },
        ChunkPolicy {
            table_header_rows: 0,
            ..policy()
        },
    ] {
        assert_eq!(
            invalid.validate().unwrap_err().code(),
            "invalid_chunk_policy"
        );
    }
}
