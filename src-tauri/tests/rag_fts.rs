use bloomery::rag::index::fts::{search, FtsSearchRequest};
use bloomery::rag::model::{
    ChunkId, EmbeddingIdentity, EmbeddingVectorBatch, KnowledgeBaseId, NewChunk,
    NewDocumentVersion, NewSourceDocument, SourceDocumentId, SourceLocation,
};
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::knowledge;
use rusqlite::Connection;

const WORKSPACE: &str = "workspace-a";
const PROFILE: &str = "11111111-1111-4111-8111-111111111111";
const MODEL: &str = "BAAI/bge-m3";

#[test]
fn fts_searches_english_grade_aliases_phrases_and_cjk_with_snippets() {
    let mut fixture = Fixture::new();
    let base = fixture.base("Steel standards");
    let document = fixture.document(base, "GB-T 1591.pdf");
    fixture.version(
        document,
        'a',
        true,
        &[
            (
                "grade",
                "Q355-B structural steel has a yield strength of 355 MPa.",
                SourceLocation::Heading {
                    path: vec!["Mechanical properties".to_string()],
                },
            ),
            (
                "cjk",
                "低合金钢材的屈服强度应符合标准要求。",
                SourceLocation::Heading {
                    path: vec!["力学性能".to_string()],
                },
            ),
        ],
    );

    let grade = fixture.search("Q355B yield strength", &[base]);
    assert_eq!(grade.len(), 1);
    assert_eq!(grade[0].chunk_id.as_str(), "grade");
    assert!(grade[0].snippet.contains("<mark>"));
    assert_eq!(grade[0].source_name, "GB-T 1591.pdf");
    assert_eq!(grade[0].title_path, "Mechanical properties");

    assert_eq!(fixture.search("\"yield strength\"", &[base]).len(), 1);
    assert_eq!(
        fixture.search("屈服强度", &[base])[0].chunk_id.as_str(),
        "cjk"
    );
}

#[test]
fn fts_filters_workspace_bases_and_inactive_versions_with_stable_ids() {
    let mut fixture = Fixture::new();
    let selected = fixture.base("Selected");
    let other = fixture.base("Other");
    let selected_document = fixture.document(selected, "selected.pdf");
    fixture.version(
        selected_document,
        'b',
        true,
        &[(
            "active",
            "continuous casting crack prevention",
            SourceLocation::TextOffsets { start: 0, end: 35 },
        )],
    );
    fixture.version(
        selected_document,
        'c',
        false,
        &[(
            "inactive",
            "obsolete ferrite guidance",
            SourceLocation::TextOffsets { start: 0, end: 25 },
        )],
    );
    let other_document = fixture.document(other, "other.pdf");
    fixture.version(
        other_document,
        'd',
        true,
        &[(
            "other-hit",
            "continuous casting crack prevention",
            SourceLocation::TextOffsets { start: 0, end: 35 },
        )],
    );

    let first = fixture.search("continuous casting", &[selected]);
    let second = fixture.search("continuous casting", &[selected]);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].chunk_id, second[0].chunk_id);
    assert_eq!(first[0].version_id, second[0].version_id);
    assert_eq!(first[0].knowledge_base_id, selected);
    assert!(fixture.search("obsolete ferrite", &[selected]).is_empty());
    assert!(fixture.search("continuous casting", &[]).is_empty());
    assert!(fixture.search("   ", &[selected]).is_empty());
    assert!(search(
        &fixture.connection,
        &FtsSearchRequest {
            workspace_id: WORKSPACE.to_string(),
            query: "continuous casting".to_string(),
            knowledge_base_ids: vec![selected],
            limit: 0,
        },
    )
    .unwrap()
    .is_empty());
}

#[test]
fn latest_migration_exposes_rich_fts_columns() {
    let fixture = Fixture::new();
    let mut statement = fixture
        .connection
        .prepare("PRAGMA table_info(knowledge_chunks_fts)")
        .unwrap();
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for required in [
        "knowledge_base_id",
        "document_id",
        "title_path",
        "source_name",
        "grade_aliases",
    ] {
        assert!(
            columns.iter().any(|column| column == required),
            "missing {required}"
        );
    }
}

#[test]
fn fts_neutralizes_sql_and_fts5_operator_payloads() {
    let mut fixture = Fixture::new();
    let base = fixture.base("Injection");
    let alpha = fixture.document(base, "alpha.pdf");
    fixture.version(
        alpha,
        'a',
        true,
        &[(
            "alpha",
            "alpha steel content",
            SourceLocation::Heading {
                path: vec!["Alpha".to_string()],
            },
        )],
    );
    let beta = fixture.document(base, "beta.pdf");
    fixture.version(
        beta,
        'b',
        true,
        &[(
            "beta",
            "beta iron content",
            SourceLocation::Heading {
                path: vec!["secret".to_string()],
            },
        )],
    );

    // 基线：两篇文档均可被各自正文词命中，"secret" 作为普通词经 title_path 列可检索。
    assert_eq!(fixture.search("steel", &[base]).len(), 1);
    assert_eq!(fixture.search("iron", &[base]).len(), 1);
    let secret_hits = fixture.search("secret", &[base]);
    assert_eq!(secret_hits.len(), 1);
    assert_eq!(secret_hits[0].source_name, "beta.pdf");

    // 经典 SQL 注入 payload 不得返回全量结果（被中和为字面短语，命中为空）。
    assert!(fixture.search("'\"OR 1=1--'", &[base]).is_empty());
    // FTS5 列过滤语法 title_path:secret 不得被解释为列过滤（否则会命中 beta.pdf）。
    assert!(fixture.search("title_path:secret", &[base]).is_empty());
    // 裸通配符 * 不得触发全量扫描。
    assert!(fixture.search("*", &[base]).is_empty());
    // FTS5 布尔/邻近运算符被双引号包裹为字面 token，不产生布尔展开。
    assert!(fixture.search("steel OR iron", &[base]).is_empty());
    assert!(fixture.search("steel NOT beta", &[base]).is_empty());
    assert!(fixture.search("steel AND iron", &[base]).is_empty());
    assert!(fixture.search("steel NEAR iron", &[base]).is_empty());
}

struct Fixture {
    connection: Connection,
}

impl Fixture {
    fn new() -> Self {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate(&mut connection).unwrap();
        Self { connection }
    }

    fn base(&mut self, name: &str) -> KnowledgeBaseId {
        knowledge::create_knowledge_base(&self.connection, WORKSPACE, name)
            .unwrap()
            .id
    }

    fn document(&mut self, knowledge_base_id: KnowledgeBaseId, name: &str) -> SourceDocumentId {
        knowledge::create_source_document(
            &self.connection,
            WORKSPACE,
            NewSourceDocument {
                knowledge_base_id,
                display_name: name.to_string(),
                source_kind: "file".to_string(),
            },
        )
        .unwrap()
        .id
    }

    fn version(
        &mut self,
        document_id: SourceDocumentId,
        hash_byte: char,
        activate: bool,
        chunks: &[(&str, &str, SourceLocation)],
    ) {
        let version = knowledge::create_document_version(
            &self.connection,
            WORKSPACE,
            NewDocumentVersion {
                document_id,
                content_sha256: hash_byte.to_string().repeat(64),
                mime_type: "application/pdf".to_string(),
                parser: "mineru".to_string(),
                parser_version: "v4".to_string(),
                chunk_policy_version: "steel-v1".to_string(),
                embedding_profile_id: PROFILE.to_string(),
                embedding_model_id: MODEL.to_string(),
                embedding_dimension: 2,
                expected_asset_count: 0,
                expected_chunk_count: chunks.len() as u32,
            },
        )
        .unwrap();
        for (ordinal, (id, text, location)) in chunks.iter().enumerate() {
            let chunk_id = ChunkId::new(*id).unwrap();
            knowledge::add_chunk(
                &self.connection,
                WORKSPACE,
                NewChunk {
                    id: chunk_id.clone(),
                    version_id: version.id,
                    ordinal: ordinal as u32,
                    text: (*text).to_string(),
                    source_location: location.clone(),
                    content_sha256: format!("{:064x}", ordinal + 1),
                    policy_version: "steel-v1".to_string(),
                },
            )
            .unwrap();
            knowledge::persist_embedding_batch(
                &mut self.connection,
                WORKSPACE,
                version.id,
                &[EmbeddingVectorBatch {
                    vector_key: format!("vector-{hash_byte}-{ordinal}"),
                    identity: EmbeddingIdentity {
                        provider_profile_id: PROFILE.to_string(),
                        model_id: MODEL.to_string(),
                        dimension: 2,
                        normalized_text_sha256: format!("{:064x}", ordinal + 100),
                        policy_version: "steel-v1".to_string(),
                    },
                    vector_blob: vec![0; 8],
                    vector_sha256: format!("{:064x}", ordinal + 200),
                    chunk_ids: vec![chunk_id],
                }],
            )
            .unwrap();
        }
        knowledge::finalize_flat_index(&mut self.connection, WORKSPACE, version.id).unwrap();
        if activate {
            knowledge::activate_document_version(
                &mut self.connection,
                WORKSPACE,
                document_id,
                version.id,
            )
            .unwrap();
        }
    }

    fn search(
        &self,
        query: &str,
        knowledge_base_ids: &[KnowledgeBaseId],
    ) -> Vec<bloomery::rag::index::fts::FtsHit> {
        search(
            &self.connection,
            &FtsSearchRequest {
                workspace_id: WORKSPACE.to_string(),
                query: query.to_string(),
                knowledge_base_ids: knowledge_base_ids.to_vec(),
                limit: 20,
            },
        )
        .unwrap()
    }
}
