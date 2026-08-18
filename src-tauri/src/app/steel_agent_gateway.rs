use crate::agent::runtime::CancellationToken;
use crate::app::compute_commands::logic::{
    self, OptimizeSteelProcessRequest, PredictSteelModelRequest, TrainSteelDatasetRequest,
};
use crate::app::knowledge_commands::logic::{
    query_local_knowledge_from_path, LocalKnowledgeQueryRequest,
};
use crate::app::task_commands::tasks::background_task_response;
use crate::models::MemoryInput;
use crate::providers::profiles::ProviderCapability;
use crate::rag::citation::EvidenceItem;
use crate::rag::index::fts::{search as search_fts, FtsHit, FtsSearchRequest};
use crate::rag::ingest::{queue_document_import, DocumentImportRequest, KnowledgeBaseTarget};
use crate::rag::model::KnowledgeBaseId;
use crate::steel::{
    read_dataset_table, DatasetPreviewRequest, SteelAgentGateway, SteelAgentGatewayFuture,
};
use crate::storage::repositories::{
    knowledge, memories, provider_profiles, settings, steel as steel_repository, steel_models,
};
use crate::storage::secrets::KeyringSecretStore;
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Clone)]
pub struct DesktopSteelAgentGateway {
    database: PathBuf,
    workspace_id: String,
}

impl DesktopSteelAgentGateway {
    pub fn new(database: PathBuf, workspace_id: impl Into<String>) -> Self {
        Self {
            database,
            workspace_id: workspace_id.into(),
        }
    }

    fn open(&self) -> Result<Connection, String> {
        let (connection, _) = crate::storage::database::open(&self.database)
            .map_err(|error| format!("open steel agent database failed: {error}"))?;
        Ok(connection)
    }

    fn get_model_status(&self, arguments: Value) -> Result<Value, String> {
        let lineage_id = text_arg(&arguments, "lineageId");
        let query = text_arg(&arguments, "query").to_ascii_lowercase();
        let connection = self.open()?;
        let mut models = if lineage_id.is_empty() {
            steel_models::list_all(&connection, &self.workspace_id)?
        } else {
            steel_models::list(&connection, &self.workspace_id, &lineage_id)?
        };
        if !query.is_empty() {
            models.retain(|model| {
                [
                    model.id.as_str(),
                    model.lineage_id.as_str(),
                    model.kind.as_str(),
                    model.source_task_id.as_deref().unwrap_or_default(),
                ]
                .join("\n")
                .to_ascii_lowercase()
                .contains(&query)
            });
        }
        let compact = models.iter().map(compact_model).collect::<Vec<_>>();
        Ok(json!({
            "success": true,
            "models": compact,
            "active_models": compact.iter().filter(|model| model["is_active"].as_bool() == Some(true)).cloned().collect::<Vec<_>>(),
        }))
    }

    async fn search_literature(&self, arguments: Value) -> Result<Value, String> {
        let query = text_arg(&arguments, "query");
        if query.is_empty() {
            return Err("query is required".to_string());
        }
        let limit = limit_arg(&arguments, 12);
        let connection = self.open()?;
        let base_ids = self.knowledge_base_ids(&connection, &arguments)?;
        drop(connection);
        if !base_ids.is_empty() {
            match self
                .search_literature_hybrid(query.clone(), base_ids.clone(), limit)
                .await
            {
                Ok(result) => return Ok(result),
                Err(error) => {
                    let connection = self.open()?;
                    let results = self.search_fts(&connection, &query, base_ids, limit)?;
                    let literature_results = results.clone();
                    return Ok(json!({
                        "success": true,
                        "mode": "local_fts",
                        "degraded_from": "local_hybrid",
                        "degradation": error,
                        "results": results,
                        "literature_results": literature_results,
                        "image_results": [],
                        "experimental_images": [],
                    }));
                }
            }
        }
        let connection = self.open()?;
        let results = self.search_fts(&connection, &query, base_ids, limit)?;
        let literature_results = results.clone();
        Ok(json!({
            "success": true,
            "mode": "local_fts",
            "results": results,
            "literature_results": literature_results,
            "image_results": [],
            "experimental_images": [],
        }))
    }

    async fn search_literature_hybrid(
        &self,
        query: String,
        knowledge_base_ids: Vec<KnowledgeBaseId>,
        limit: usize,
    ) -> Result<Value, String> {
        let pack = query_local_knowledge_from_path(
            self.database.clone(),
            &self.workspace_id,
            &KeyringSecretStore,
            LocalKnowledgeQueryRequest {
                query,
                knowledge_base_ids,
                lexical_limit: (limit * 3).min(50),
                dense_limit: (limit * 3).min(50),
                candidate_limit: limit,
                rrf_k: 60,
                rerank_limit: limit,
            },
        )
        .await?;
        let results = pack
            .evidence
            .into_iter()
            .map(compact_evidence_item)
            .collect::<Vec<_>>();
        let literature_results = results.clone();
        Ok(json!({
            "success": true,
            "mode": "local_hybrid",
            "evidence_pack_id": pack.id,
            "created_at": pack.created_at,
            "results": results,
            "literature_results": literature_results,
            "image_results": [],
            "experimental_images": [],
        }))
    }

    fn read_literature_section(&self, arguments: Value) -> Result<Value, String> {
        let query = text_arg(&arguments, "query");
        if query.is_empty() {
            return Err("query is required".to_string());
        }
        let mode = non_empty_or(&text_arg(&arguments, "mode"), "section");
        let document_hint = text_arg(&arguments, "document_hint");
        let folder_hint = text_arg(&arguments, "folder_hint");
        let chapter_number = arguments.get("chapter_number").and_then(Value::as_u64);
        let mut expanded = format!("{query} {mode}");
        if !document_hint.is_empty() {
            expanded.push(' ');
            expanded.push_str(&document_hint);
        }
        if !folder_hint.is_empty() {
            expanded.push(' ');
            expanded.push_str(&folder_hint);
        }
        if let Some(number) = chapter_number {
            expanded.push(' ');
            expanded.push_str(&number.to_string());
        }
        let connection = self.open()?;
        let base_ids = self.knowledge_base_ids(&connection, &arguments)?;
        let results =
            self.search_fts(&connection, &expanded, base_ids, limit_arg(&arguments, 8))?;
        if results.is_empty() {
            return Ok(json!({
                "success": false,
                "error": "未找到匹配的本地知识库正文。",
                "results": [],
            }));
        }
        let chunks = chunks_from_literature_results(
            &results,
            char_limit_arg(&arguments, "max_chars", 12_000, 60_000),
        );
        let part = part_arg(&arguments, chunks.len());
        let first = &results[0];
        Ok(json!({
            "success": true,
            "answer_type": "document_section",
            "mode": "local_fts_section",
            "section_mode": mode,
            "document": first.get("source_name").and_then(Value::as_str).unwrap_or_default(),
            "section_title": first.get("title_path").and_then(Value::as_str).unwrap_or_default(),
            "content": chunks.get(part - 1).cloned().unwrap_or_default(),
            "part": part,
            "total_parts": chunks.len(),
            "has_more": part < chunks.len(),
            "results": results,
        }))
    }

    fn query_standard(&self, arguments: Value, standard: &str) -> Result<Value, String> {
        let query = text_arg(&arguments, "query");
        if query.is_empty() {
            return Err("query is required".to_string());
        }
        let expanded = match standard {
            "composition" => format!("{query} 化学成分 成分 标准 composition"),
            "process" => format!("{query} 工艺 参数 标准 process"),
            _ => query,
        };
        let connection = self.open()?;
        let base_ids = self.knowledge_base_ids(&connection, &arguments)?;
        let results =
            self.search_fts(&connection, &expanded, base_ids, limit_arg(&arguments, 10))?;
        let records = results.clone();
        Ok(json!({
            "success": true,
            "standard": standard,
            "mode": "local_fts",
            "results": results,
            "records": records,
            "columns": [],
        }))
    }

    fn query_production_data(&self, arguments: Value) -> Result<Value, String> {
        let filters = ProductionRowFilters::from_arguments(&arguments);
        let limit = limit_arg(&arguments, 12);
        let connection = self.open()?;
        let mut records = Vec::new();
        let datasets = steel_repository::list(&connection, &self.workspace_id)?
            .into_iter()
            .filter_map(|dataset| {
                let headers = dataset
                    .preview
                    .columns
                    .iter()
                    .map(|column| column.name.clone())
                    .collect::<Vec<_>>();
                let matched_rows = dataset_sample_rows(&dataset, &filters, 8);
                if filters.has_any()
                    && matched_rows.rows.is_empty()
                    && !dataset_matches_terms(&dataset, &filters.text_terms)
                {
                    return None;
                }
                records.extend(
                    matched_rows
                        .rows
                        .iter()
                        .map(|row| compact_production_record(&dataset, row)),
                );
                Some(json!({
                    "id": dataset.id,
                    "source_name": dataset.source_name,
                    "format": dataset.format,
                    "row_count": dataset.row_count,
                    "column_count": dataset.column_count,
                    "mapping_state": dataset.mapping_state,
                    "updated_at": dataset.updated_at,
                    "columns": dataset.columns.into_iter().map(|column| json!({
                        "ordinal": column.ordinal,
                        "name": column.original_name,
                        "canonical_field": column.canonical_field,
                        "unit": column.unit,
                        "min": column.min,
                        "max": column.max,
                    })).collect::<Vec<_>>(),
                    "headers": headers,
                    "sample_rows": matched_rows.rows,
                    "sample_row_source": matched_rows.source,
                    "sample_row_error": matched_rows.error,
                    "filters": filters.to_json(),
                }))
            })
            .take(limit)
            .collect::<Vec<_>>();
        Ok(json!({
            "success": true,
            "mode": "local_steel_datasets",
            "datasets": datasets,
            "records": records,
        }))
    }

    fn match_coil(&self, arguments: Value) -> Result<Value, String> {
        let targets = coil_match_targets(&arguments);
        if targets.is_empty() {
            let mut result = self.query_production_data(arguments)?;
            result["match_strategy"] = json!("local_preview_rows");
            return Ok(result);
        }
        let tolerance = numeric_arg(&arguments, "tolerance").unwrap_or(50.0).abs();
        let limit = limit_arg(&arguments, 8);
        let connection = self.open()?;
        let mut matches = Vec::new();
        for dataset in steel_repository::list(&connection, &self.workspace_id)? {
            let columns = targets
                .iter()
                .filter_map(|(field, target)| {
                    dataset
                        .columns
                        .iter()
                        .find(|column| column.canonical_field.as_deref() == Some(*field))
                        .map(|column| (*field, column.ordinal, *target))
                })
                .collect::<Vec<_>>();
            if columns.len() != targets.len() {
                continue;
            }
            let rows = dataset_rows(&dataset);
            for (row_index, row) in rows.rows.iter().enumerate() {
                let mut distance = 0.0;
                let mut values = serde_json::Map::new();
                for (field, ordinal, target) in &columns {
                    let Some(value) = row.get(*ordinal).and_then(|cell| parse_number(cell)) else {
                        distance = f64::INFINITY;
                        break;
                    };
                    distance += (value - target).abs();
                    values.insert((*field).to_string(), json!(value));
                }
                if distance.is_finite() && distance <= tolerance * columns.len() as f64 {
                    matches.push(json!({
                        "dataset_id": dataset.id,
                        "source_name": dataset.source_name,
                        "row_number": row_index + 2,
                        "row": row,
                        "row_source": rows.source,
                        "source_error": rows.error.clone(),
                        "values": values,
                        "score": distance,
                    }));
                }
            }
        }
        matches.sort_by(|left, right| {
            left["score"]
                .as_f64()
                .partial_cmp(&right["score"].as_f64())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches.truncate(limit);
        Ok(json!({
            "success": true,
            "mode": "local_preview_rows",
            "matches": matches,
            "total_matched": matches.len(),
        }))
    }

    fn ask_llm_with_context(&self, arguments: Value) -> Result<Value, String> {
        let query = text_arg(&arguments, "query");
        let evidence = arguments
            .get("evidence")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let evidence_count = evidence.len();
        Ok(json!({
            "success": true,
            "mode": "local_agent_synthesis_context",
            "query": query,
            "evidence_count": evidence_count,
            "message": "已整理当前证据；Bloomery 桌面端由本地 AgentLoop 继续生成最终回答，不在工具内部递归调用 LLM。",
        }))
    }

    fn process_literature(&self, arguments: Value) -> Result<Value, String> {
        let file_path = text_arg(&arguments, "file_path");
        if file_path.is_empty() {
            return Ok(json!({
                "success": false,
                "requires_user_action": true,
                "message": "Agent 已识别文献处理意图。请在知识库页面选择本地 PDF/Markdown/Office 文件后导入。",
            }));
        }
        let mut connection = self.open()?;
        let embedding_profile_id = match self.resolve_profile_id(
            &connection,
            &arguments,
            "embedding_profile_id",
            ProviderCapability::Embedding,
        )? {
            Some(id) => id,
            None => {
                return Ok(json!({
                    "success": false,
                    "requires_user_action": true,
                    "error": "default embedding provider is not configured",
                    "message": "请先在设置里配置默认 Embedding 提供商（例如 SiliconFlow bge-m3）和 API Key。",
                }))
            }
        };
        let mineru_profile_id = self.resolve_profile_id(
            &connection,
            &arguments,
            "mineru_profile_id",
            ProviderCapability::DocumentParser,
        )?;
        let content_root = self
            .database
            .parent()
            .map(std::path::Path::to_path_buf)
            .ok_or_else(|| "resolve RAG content root failed".to_string())?;
        let knowledge_base = self.knowledge_base_target(&connection, &arguments)?;
        let response = queue_document_import(
            &mut connection,
            &self.workspace_id,
            &KeyringSecretStore,
            &content_root,
            DocumentImportRequest {
                source_path: PathBuf::from(file_path),
                knowledge_base,
                mineru_profile_id,
                embedding_profile_id,
                embedding_dimension: arguments
                    .get("embedding_dimension")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .unwrap_or(1024),
            },
        )?;
        Ok(json!({
            "success": true,
            "task_id": response.task_id,
            "knowledge_base_id": response.knowledge_base_id,
            "document_id": response.document_id,
            "version_id": response.version_id,
            "ingest_attempt_id": response.ingest_attempt_id,
            "duplicate_content": response.duplicate_content,
        }))
    }

    fn export_data(&self, arguments: Value) -> Result<Value, String> {
        Ok(json!({
            "success": false,
            "requires_user_action": true,
            "format": non_empty_or(&text_arg(&arguments, "format"), "xlsx"),
            "message": "Agent 已识别导出意图。请在对应结果区点击导出，以避免后台自动生成文件。",
        }))
    }

    fn knowledge_base_ids(
        &self,
        connection: &Connection,
        arguments: &Value,
    ) -> Result<Vec<KnowledgeBaseId>, String> {
        if let Some(values) = arguments
            .get("knowledge_base_ids")
            .and_then(Value::as_array)
        {
            let ids = values
                .iter()
                .filter_map(Value::as_str)
                .map(KnowledgeBaseId::from_str)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;
            if !ids.is_empty() {
                return Ok(ids);
            }
        }
        knowledge::list_knowledge_bases(connection, &self.workspace_id)
            .map(|bases| bases.into_iter().map(|base| base.id).collect())
    }

    fn search_fts(
        &self,
        connection: &Connection,
        query: &str,
        knowledge_base_ids: Vec<KnowledgeBaseId>,
        limit: usize,
    ) -> Result<Vec<Value>, String> {
        search_fts(
            connection,
            &FtsSearchRequest {
                workspace_id: self.workspace_id.clone(),
                query: query.to_string(),
                knowledge_base_ids,
                limit,
            },
        )
        .map_err(|error| error.to_string())
        .map(|hits| hits.into_iter().map(compact_fts_hit).collect())
    }

    fn predict_performance(&self, arguments: Value) -> Result<Value, String> {
        let arguments = normalize_compute_arguments(arguments);
        if looks_like_web_prediction(&arguments) && !has_local_prediction_shape(&arguments) {
            return Ok(local_compute_setup_required("预测力学性能"));
        }
        let request: PredictSteelModelRequest = serde_json::from_value(arguments)
            .map_err(|error| format!("invalid prediction request: {error}"))?;
        let training_task_id = uuid::Uuid::parse_str(&request.training_task_id)
            .map_err(|error| format!("invalid training task ID: {error}"))?;
        let mut connection = self.open()?;
        let task = logic::predict_steel_model_on_connection(
            &mut connection,
            &self.workspace_id,
            &request,
            training_task_id,
        )?;
        Ok(json!({
            "success": true,
            "task": background_task_response(task),
        }))
    }

    fn optimize_process(&self, arguments: Value) -> Result<Value, String> {
        let arguments = normalize_compute_arguments(arguments);
        if looks_like_web_optimization(&arguments) && !has_local_optimization_shape(&arguments) {
            return Ok(local_compute_setup_required("工艺参数优化"));
        }
        let request: OptimizeSteelProcessRequest = serde_json::from_value(arguments)
            .map_err(|error| format!("invalid optimization request: {error}"))?;
        let training_task_id = uuid::Uuid::parse_str(&request.training_task_id)
            .map_err(|error| format!("invalid training task ID: {error}"))?;
        let mut connection = self.open()?;
        let task =
            logic::submit_optimization_on_connection(&mut connection, &request, training_task_id)?;
        Ok(json!({
            "success": true,
            "task": background_task_response(task),
        }))
    }

    fn start_training(&self, arguments: Value) -> Result<Value, String> {
        let arguments = normalize_compute_arguments(arguments);
        if !has_local_training_shape(&arguments) {
            return Ok(local_compute_setup_required("启动模型训练"));
        }
        let request: TrainSteelDatasetRequest = serde_json::from_value(arguments)
            .map_err(|error| format!("invalid training request: {error}"))?;
        let mut connection = self.open()?;
        let task = logic::train_steel_dataset_on_connection(
            &mut connection,
            &self.workspace_id,
            &request,
        )?;
        Ok(json!({
            "success": true,
            "task": background_task_response(task),
        }))
    }

    fn remember_memory(&self, arguments: Value) -> Result<Value, String> {
        let summary = text_arg(&arguments, "summary");
        if summary.is_empty() {
            return Err("summary is required".to_string());
        }
        let title = text_arg(&arguments, "title");
        let tags_json = serde_json::to_string(
            arguments
                .get("keywords")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .as_slice(),
        )
        .map_err(|error| error.to_string())?;
        let mut connection = self.open()?;
        let saved = memories::save(
            &mut connection,
            &self.workspace_id,
            MemoryInput {
                id: None,
                scope: "user".to_string(),
                r#type: non_empty_or(&text_arg(&arguments, "memory_type"), "general"),
                title: non_empty_or(&title, &summary.chars().take(40).collect::<String>()),
                description: String::new(),
                body: summary,
                tags_json,
                enabled: true,
            },
        )?;
        Ok(json!({
            "success": true,
            "memory_id": saved.id,
            "item": saved,
        }))
    }

    fn read_memory(&self, arguments: Value) -> Result<Value, String> {
        let memory_id = text_arg(&arguments, "memory_id");
        if memory_id.is_empty() {
            return Err("memory_id is required".to_string());
        }
        let connection = self.open()?;
        let item = memories::get(&connection, &self.workspace_id, &memory_id)?;
        Ok(json!({
            "success": item.is_some(),
            "memory_id": memory_id,
            "item": item,
            "error": if item.is_some() { Value::Null } else { json!("memory not found") },
        }))
    }

    fn search_memory(&self, arguments: Value) -> Result<Value, String> {
        let query = text_arg(&arguments, "query");
        if query.is_empty() {
            return Err("query is required".to_string());
        }
        self.list_memory(json!({
            "query": query,
            "memory_type": text_arg(&arguments, "memory_type"),
        }))
    }

    fn list_memory(&self, arguments: Value) -> Result<Value, String> {
        let query = text_arg(&arguments, "query");
        let memory_type = text_arg(&arguments, "memory_type");
        let connection = self.open()?;
        let mut items = memories::list(&connection, &self.workspace_id, false, &query)?;
        if !memory_type.is_empty() {
            items.retain(|item| item.r#type == memory_type);
        }
        Ok(json!({
            "success": true,
            "items": items,
        }))
    }

    fn forget_memory(&self, arguments: Value) -> Result<Value, String> {
        let memory_id = text_arg(&arguments, "memory_id");
        if memory_id.is_empty() {
            return Err("memory_id is required".to_string());
        }
        let mut connection = self.open()?;
        memories::archive(&mut connection, &self.workspace_id, &memory_id)?;
        Ok(json!({
            "success": true,
            "forgotten": true,
            "memory_id": memory_id,
        }))
    }

    fn resolve_profile_id(
        &self,
        connection: &Connection,
        arguments: &Value,
        key: &str,
        capability: ProviderCapability,
    ) -> Result<Option<uuid::Uuid>, String> {
        let explicit = text_arg(arguments, key);
        if !explicit.is_empty() {
            return uuid::Uuid::parse_str(&explicit)
                .map(Some)
                .map_err(|error| format!("invalid {key}: {error}"));
        }
        if let Some(id) = self.profile_id_from_retrieval_setting(connection, key)? {
            return Ok(Some(id));
        }
        provider_profiles::get_default_record(connection, &self.workspace_id, capability)
            .map(|record| record.map(|record| record.profile.id))
    }

    fn profile_id_from_retrieval_setting(
        &self,
        connection: &Connection,
        key: &str,
    ) -> Result<Option<uuid::Uuid>, String> {
        let Some(raw) = settings::get(connection, &self.workspace_id, "onboarding.retrieval")?
        else {
            return Ok(None);
        };
        let value = serde_json::from_str::<Value>(&raw).map_err(|error| error.to_string())?;
        let id = value.get(key).and_then(Value::as_str).unwrap_or_default();
        if id.trim().is_empty() {
            return Ok(None);
        }
        uuid::Uuid::parse_str(id)
            .map(Some)
            .map_err(|error| format!("invalid {key} in retrieval settings: {error}"))
    }

    fn knowledge_base_target(
        &self,
        connection: &Connection,
        arguments: &Value,
    ) -> Result<KnowledgeBaseTarget, String> {
        let id = text_arg(arguments, "knowledge_base_id");
        if !id.is_empty() {
            return KnowledgeBaseId::from_str(&id)
                .map(|id| KnowledgeBaseTarget::Existing { id })
                .map_err(|error| error.to_string());
        }
        let name = non_empty_or(&text_arg(arguments, "knowledge_base_name"), "钢铁文献");
        let existing = knowledge::list_knowledge_bases(connection, &self.workspace_id)?
            .into_iter()
            .find(|base| base.name == name);
        Ok(match existing {
            Some(base) => KnowledgeBaseTarget::Existing { id: base.id },
            None => KnowledgeBaseTarget::Create { name },
        })
    }
}

impl SteelAgentGateway for DesktopSteelAgentGateway {
    fn execute(
        &self,
        tool_name: &'static str,
        arguments: Value,
        cancellation: CancellationToken,
    ) -> SteelAgentGatewayFuture {
        let gateway = self.clone();
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err("tool execution was cancelled".to_string());
            }
            match tool_name {
                "search_literature" => gateway.search_literature(arguments).await,
                "read_literature_section" => gateway.read_literature_section(arguments),
                "query_production_data" => gateway.query_production_data(arguments),
                "query_composition_standard" => gateway.query_standard(arguments, "composition"),
                "query_process_standard" => gateway.query_standard(arguments, "process"),
                "ask_llm_with_context" => gateway.ask_llm_with_context(arguments),
                "get_model_status" => gateway.get_model_status(arguments),
                "predict_performance" => gateway.predict_performance(arguments),
                "optimize_process" => gateway.optimize_process(arguments),
                "match_coil" => gateway.match_coil(arguments),
                "start_training" => gateway.start_training(arguments),
                "process_literature" => gateway.process_literature(arguments),
                "export_data" => gateway.export_data(arguments),
                "remember_memory" => gateway.remember_memory(arguments),
                "read_memory" => gateway.read_memory(arguments),
                "search_memory" => gateway.search_memory(arguments),
                "list_memory" => gateway.list_memory(arguments),
                "forget_memory" => gateway.forget_memory(arguments),
                _ => Err(format!("unsupported steel agent tool: {tool_name}")),
            }
        })
    }
}

fn text_arg(arguments: &Value, key: &str) -> String {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn limit_arg(arguments: &Value, fallback: usize) -> usize {
    ["limit", "top_k"]
        .into_iter()
        .find_map(|key| {
            arguments
                .get(key)
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
        })
        .unwrap_or(fallback)
        .clamp(1, 50)
}

fn char_limit_arg(arguments: &Value, key: &str, fallback: usize, maximum: usize) -> usize {
    arguments
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(fallback)
        .clamp(1, maximum)
}

fn part_arg(arguments: &Value, total_parts: usize) -> usize {
    arguments
        .get("part")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(1)
        .clamp(1, total_parts.max(1))
}

fn non_empty_or(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value.trim().to_string()
    }
}

fn compact_fts_hit(hit: FtsHit) -> Value {
    json!({
        "knowledge_base_id": hit.knowledge_base_id,
        "document_id": hit.document_id,
        "version_id": hit.version_id,
        "chunk_id": hit.chunk_id,
        "source_name": hit.source_name,
        "title_path": hit.title_path,
        "snippet": hit.snippet,
        "text": truncate(&hit.text, 1200),
        "score": hit.bm25_score,
        "cjk_fallback": hit.cjk_fallback,
    })
}

fn compact_evidence_item(item: EvidenceItem) -> Value {
    json!({
        "citation_number": item.citation_number,
        "knowledge_base_id": item.chunk.knowledge_base_id,
        "document_id": item.chunk.document_id,
        "version_id": item.chunk.version_id,
        "chunk_id": item.chunk.chunk_id,
        "source_name": item.chunk.source_name,
        "source_location": item.chunk.source_location,
        "text": truncate(&item.chunk.text, 1200),
        "lexical_rank": item.chunk.lexical_rank,
        "dense_rank": item.chunk.dense_rank,
        "score": item.chunk.rerank_score.map(f64::from).unwrap_or(item.chunk.rrf_score),
        "rrf_score": item.chunk.rrf_score,
        "rerank_score": item.chunk.rerank_score,
        "assets": item.assets,
    })
}

fn chunks_from_literature_results(results: &[Value], max_chars: usize) -> Vec<String> {
    let content = results
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n\n");
    chunk_by_chars(&content, max_chars)
}

fn chunk_by_chars(value: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut chunk = String::new();
    let mut count = 0usize;
    for character in value.chars() {
        if count >= max_chars {
            chunks.push(chunk);
            chunk = String::new();
            count = 0;
        }
        chunk.push(character);
        count += 1;
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    chunks
}

fn compact_model(model: &steel_models::SteelModelRecord) -> Value {
    json!({
        "id": model.id,
        "lineage_id": model.lineage_id,
        "kind": model.kind,
        "version": model.version,
        "source_task_id": model.source_task_id,
        "model_sha256": model.model_sha256,
        "is_active": model.is_active,
        "created_at": model.created_at,
        "manifest": serde_json::from_str::<Value>(&model.manifest_json).unwrap_or(Value::Null),
    })
}

fn compact_production_record(
    dataset: &steel_repository::SteelDatasetRecord,
    row: &[String],
) -> Value {
    let values = dataset
        .columns
        .iter()
        .filter_map(|column| {
            let field = column.canonical_field.as_deref()?;
            let cell = row.get(column.ordinal)?;
            Some((field.to_string(), production_cell_value(cell)))
        })
        .collect::<Map<_, _>>();
    json!({
        "dataset_id": dataset.id,
        "source_name": dataset.source_name,
        "row": row,
        "values": values,
    })
}

fn production_cell_value(cell: &str) -> Value {
    parse_number(cell).map_or_else(|| json!(cell), |value| json!(value))
}

#[derive(Clone, Debug)]
struct ProductionRowFilters {
    text_terms: Vec<String>,
    numeric_ranges: Vec<NumericRangeFilter>,
}

impl ProductionRowFilters {
    fn from_arguments(arguments: &Value) -> Self {
        let structured_terms = ["steel_mark", "steel_grade"]
            .into_iter()
            .filter_map(|key| {
                let value = text_arg(arguments, key).to_ascii_lowercase();
                (!value.is_empty()).then_some(value)
            })
            .collect::<Vec<_>>();
        let numeric_ranges = production_numeric_ranges(arguments);
        let query = text_arg(arguments, "query").to_ascii_lowercase();
        let text_terms =
            if structured_terms.is_empty() && numeric_ranges.is_empty() && !query.is_empty() {
                vec![query]
            } else {
                structured_terms
            };
        Self {
            text_terms,
            numeric_ranges,
        }
    }

    fn has_any(&self) -> bool {
        !self.text_terms.is_empty() || !self.numeric_ranges.is_empty()
    }

    fn to_json(&self) -> Value {
        json!({
            "text_terms": self.text_terms,
            "numeric_ranges": self.numeric_ranges.iter().map(|range| json!({
                "field": range.canonical_field,
                "min": range.min,
                "max": range.max,
            })).collect::<Vec<_>>(),
        })
    }
}

#[derive(Clone, Debug)]
struct NumericRangeFilter {
    canonical_field: &'static str,
    min: Option<f64>,
    max: Option<f64>,
}

fn production_numeric_ranges(arguments: &Value) -> Vec<NumericRangeFilter> {
    [
        (
            "slab_width",
            &["slab_width_min"][..],
            &["slab_width_max"][..],
        ),
        (
            "slab_thickness",
            &["slab_thickness_min"][..],
            &["slab_thickness_max"][..],
        ),
        (
            "yield_strength",
            &["yield_rp02_min", "yield_strength_min"][..],
            &["yield_rp02_max", "yield_strength_max"][..],
        ),
        (
            "tensile_strength",
            &["tensile_strength_min"][..],
            &["tensile_strength_max"][..],
        ),
        (
            "elongation",
            &["elongation_min"][..],
            &["elongation_max"][..],
        ),
    ]
    .into_iter()
    .filter_map(|(canonical_field, min_keys, max_keys)| {
        let min = numeric_arg_any(arguments, min_keys);
        let max = numeric_arg_any(arguments, max_keys);
        (min.is_some() || max.is_some()).then_some(NumericRangeFilter {
            canonical_field,
            min,
            max,
        })
    })
    .collect()
}

fn dataset_matches_terms(dataset: &steel_repository::SteelDatasetRecord, terms: &[String]) -> bool {
    !terms.is_empty()
        && terms.iter().all(|term| {
            [
                dataset.id.as_str(),
                dataset.source_name.as_str(),
                dataset.format.as_str(),
                dataset.mapping_state.as_str(),
            ]
            .join("\n")
            .to_ascii_lowercase()
            .contains(term)
                || dataset.columns.iter().any(|column| {
                    [
                        column.original_name.as_str(),
                        column.canonical_field.as_deref().unwrap_or_default(),
                        column.unit.as_deref().unwrap_or_default(),
                    ]
                    .join("\n")
                    .to_ascii_lowercase()
                    .contains(term)
                })
                || dataset
                    .preview
                    .sample_rows
                    .iter()
                    .any(|row| row_matches_query(row, term))
        })
}

fn matching_sample_rows(
    dataset: &steel_repository::SteelDatasetRecord,
    rows: &[Vec<String>],
    filters: &ProductionRowFilters,
    limit: usize,
) -> Vec<Vec<String>> {
    rows.iter()
        .filter(|row| row_matches_filters(dataset, row, filters))
        .take(limit)
        .cloned()
        .collect()
}

struct DatasetRows {
    rows: Vec<Vec<String>>,
    source: &'static str,
    error: Option<String>,
}

fn dataset_sample_rows(
    dataset: &steel_repository::SteelDatasetRecord,
    filters: &ProductionRowFilters,
    limit: usize,
) -> DatasetRows {
    let mut rows = dataset_rows(dataset);
    rows.rows = matching_sample_rows(dataset, &rows.rows, filters, limit);
    rows
}

fn dataset_rows(dataset: &steel_repository::SteelDatasetRecord) -> DatasetRows {
    let sheet = (!dataset.selected_sheet.trim().is_empty()).then(|| dataset.selected_sheet.clone());
    match read_dataset_table(&DatasetPreviewRequest {
        source_path: dataset.source_path.clone(),
        sheet,
    }) {
        Ok(table) => DatasetRows {
            rows: table.rows,
            source: "source_file",
            error: None,
        },
        Err(error) => DatasetRows {
            rows: dataset.preview.sample_rows.clone(),
            source: "preview",
            error: Some(error),
        },
    }
}

fn row_matches_query(row: &[String], query: &str) -> bool {
    row.iter()
        .any(|cell| cell.to_ascii_lowercase().contains(query))
}

fn row_matches_filters(
    dataset: &steel_repository::SteelDatasetRecord,
    row: &[String],
    filters: &ProductionRowFilters,
) -> bool {
    filters
        .text_terms
        .iter()
        .all(|term| row_matches_query(row, term))
        && filters
            .numeric_ranges
            .iter()
            .all(|range| row_matches_numeric_range(dataset, row, range))
}

fn row_matches_numeric_range(
    dataset: &steel_repository::SteelDatasetRecord,
    row: &[String],
    range: &NumericRangeFilter,
) -> bool {
    let Some(ordinal) = dataset
        .columns
        .iter()
        .find(|column| column.canonical_field.as_deref() == Some(range.canonical_field))
        .map(|column| column.ordinal)
    else {
        return false;
    };
    let Some(value) = row.get(ordinal).and_then(|cell| parse_number(cell)) else {
        return false;
    };
    range.min.is_none_or(|minimum| value >= minimum)
        && range.max.is_none_or(|maximum| value <= maximum)
}

fn coil_match_targets(arguments: &Value) -> Vec<(&'static str, f64)> {
    [
        (
            "yield_strength",
            &["yield_strength", "target_yield", "yield_rp02_min"][..],
        ),
        (
            "tensile_strength",
            &["tensile_strength", "target_tensile", "tensile_strength_min"][..],
        ),
        (
            "elongation",
            &["elongation", "target_elongation", "elongation_min"][..],
        ),
    ]
    .into_iter()
    .filter_map(|(field, keys)| numeric_arg_any(arguments, keys).map(|value| (field, value)))
    .collect()
}

fn numeric_arg(arguments: &Value, key: &str) -> Option<f64> {
    arguments
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
}

fn numeric_arg_any(arguments: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| numeric_arg(arguments, key))
}

fn normalize_compute_arguments(arguments: Value) -> Value {
    let mut object = match arguments {
        Value::Object(object) => object,
        other => return other,
    };
    if let Some(params) = object.get("params").and_then(Value::as_object) {
        let params = params
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>();
        for (key, value) in params {
            object.entry(key).or_insert(value);
        }
    }
    for (from, to) in [
        ("dataset_id", "datasetId"),
        ("training_task_id", "trainingTaskId"),
        ("feature_values", "featureValues"),
        ("target_column", "targetColumn"),
        ("feature_columns", "featureColumns"),
        ("split_policy", "splitPolicy"),
        ("objective_columns", "objectiveColumns"),
        ("fixed_values", "fixedValues"),
    ] {
        copy_alias(&mut object, from, to);
    }
    Value::Object(object)
}

fn copy_alias(object: &mut Map<String, Value>, from: &str, to: &str) {
    if object.contains_key(to) {
        return;
    }
    if let Some(value) = object.get(from).cloned() {
        object.insert(to.to_string(), value);
    }
}

fn has_local_prediction_shape(arguments: &Value) -> bool {
    has_keys(arguments, &["datasetId", "trainingTaskId", "featureValues"])
}

fn has_local_optimization_shape(arguments: &Value) -> bool {
    has_keys(
        arguments,
        &[
            "datasetId",
            "trainingTaskId",
            "direction",
            "objectiveColumns",
            "bounds",
            "trials",
        ],
    )
}

fn has_local_training_shape(arguments: &Value) -> bool {
    has_keys(arguments, &["datasetId", "targetColumn", "featureColumns"])
}

fn has_keys(arguments: &Value, keys: &[&str]) -> bool {
    keys.iter()
        .all(|key| !arguments.get(*key).is_none_or(Value::is_null))
}

fn looks_like_web_prediction(arguments: &Value) -> bool {
    arguments.get("params").is_some()
}

fn looks_like_web_optimization(arguments: &Value) -> bool {
    arguments.get("filters").is_some() || arguments.get("context").is_some()
}

fn local_compute_setup_required(action: &str) -> Value {
    json!({
        "success": false,
        "requires_user_action": true,
        "action": action,
        "message": format!(
            "{action} 已切换为 Bloomery 本地计算流程。请先在生产数据页面导入数据集、完成字段映射和本地训练，再从训练结果发起该操作；桌面端不会调用 Web 云端模型。"
        ),
    })
}

fn parse_number(value: &str) -> Option<f64> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

fn truncate(value: &str, limit: usize) -> String {
    let mut result = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        result.push('…');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steel::{preview_dataset, DatasetColumnPreview, DatasetPreview};

    fn execute(
        gateway: &DesktopSteelAgentGateway,
        tool_name: &'static str,
        arguments: Value,
    ) -> Result<Value, String> {
        tauri::async_runtime::block_on(<DesktopSteelAgentGateway as SteelAgentGateway>::execute(
            gateway,
            tool_name,
            arguments,
            CancellationToken::new(|| false),
        ))
    }

    #[test]
    fn process_literature_with_file_requires_embedding_configuration() {
        let path = std::env::temp_dir().join(format!(
            "bloomery-steel-agent-gateway-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let (_connection, _) = crate::storage::database::open(&path).expect("open migrated db");
        let gateway = DesktopSteelAgentGateway::new(path.clone(), "local");

        let result = execute(
            &gateway,
            "process_literature",
            json!({"file_path": "F:/does-not-need-to-exist.pdf"}),
        )
        .expect("structured response");

        assert_eq!(result["success"], json!(false));
        assert_eq!(
            result["error"],
            json!("default embedding provider is not configured")
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn search_literature_returns_web_compatible_result_aliases() {
        let path = std::env::temp_dir().join(format!(
            "bloomery-steel-agent-gateway-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let (_connection, _) = crate::storage::database::open(&path).expect("open migrated db");
        let gateway = DesktopSteelAgentGateway::new(path.clone(), "local");

        let result = execute(&gateway, "search_literature", json!({"query": "Q355B"}))
            .expect("search literature");

        assert_eq!(result["success"], json!(true));
        assert_eq!(result["results"], json!([]));
        assert_eq!(result["literature_results"], json!([]));
        assert_eq!(result["image_results"], json!([]));
        assert_eq!(result["experimental_images"], json!([]));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn standard_queries_return_web_compatible_records_alias() {
        let path = std::env::temp_dir().join(format!(
            "bloomery-steel-agent-gateway-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let (_connection, _) = crate::storage::database::open(&path).expect("open migrated db");
        let gateway = DesktopSteelAgentGateway::new(path.clone(), "local");

        let result = execute(
            &gateway,
            "query_composition_standard",
            json!({"query": "Q355B"}),
        )
        .expect("query composition standard");

        assert_eq!(result["success"], json!(true));
        assert_eq!(result["results"], json!([]));
        assert_eq!(result["records"], json!([]));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn read_literature_section_returns_web_style_content_chunk() {
        let path = std::env::temp_dir().join(format!(
            "bloomery-steel-agent-gateway-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let (mut connection, _) = crate::storage::database::open(&path).expect("open migrated db");
        let base = knowledge::create_knowledge_base(&connection, "local", "Steel standards")
            .expect("create base")
            .id;
        let document = knowledge::create_source_document(
            &connection,
            "local",
            crate::rag::model::NewSourceDocument {
                knowledge_base_id: base,
                display_name: "GB-T 1591.pdf".to_string(),
                source_kind: "file".to_string(),
            },
        )
        .expect("create document");
        let version = knowledge::create_document_version(
            &connection,
            "local",
            crate::rag::model::NewDocumentVersion {
                document_id: document.id,
                content_sha256: "a".repeat(64),
                mime_type: "application/pdf".to_string(),
                parser: "mineru".to_string(),
                parser_version: "v4".to_string(),
                chunk_policy_version: "steel-v1".to_string(),
                embedding_profile_id: "11111111-1111-4111-8111-111111111111".to_string(),
                embedding_model_id: "BAAI/bge-m3".to_string(),
                embedding_dimension: 2,
                expected_asset_count: 0,
                expected_chunk_count: 1,
            },
        )
        .expect("create version");
        let chunk_id = crate::rag::model::ChunkId::new("chapter-2").expect("chunk id");
        knowledge::add_chunk(
            &connection,
            "local",
            crate::rag::model::NewChunk {
                id: chunk_id.clone(),
                version_id: version.id,
                ordinal: 0,
                text: "section 2 Q355B 工艺窗口：控轧温度、卷取温度和冷却速率需要结合厚度校核。"
                    .to_string(),
                source_location: crate::rag::model::SourceLocation::Heading {
                    path: vec!["第二章 工艺窗口".to_string()],
                },
                content_sha256: "b".repeat(64),
                policy_version: "steel-v1".to_string(),
            },
        )
        .expect("add chunk");
        knowledge::persist_embedding_batch(
            &mut connection,
            "local",
            version.id,
            &[crate::rag::model::EmbeddingVectorBatch {
                vector_key: "vector-a-0".to_string(),
                identity: crate::rag::model::EmbeddingIdentity {
                    provider_profile_id: "11111111-1111-4111-8111-111111111111".to_string(),
                    model_id: "BAAI/bge-m3".to_string(),
                    dimension: 2,
                    normalized_text_sha256: "c".repeat(64),
                    policy_version: "steel-v1".to_string(),
                },
                vector_blob: vec![0; 8],
                vector_sha256: "d".repeat(64),
                chunk_ids: vec![chunk_id],
            }],
        )
        .expect("persist embedding");
        knowledge::finalize_flat_index(&mut connection, "local", version.id)
            .expect("finalize index");
        knowledge::activate_document_version(&mut connection, "local", document.id, version.id)
            .expect("activate");
        drop(connection);

        let gateway = DesktopSteelAgentGateway::new(path.clone(), "local");
        let result = execute(
            &gateway,
            "read_literature_section",
            json!({
                "query": "Q355B 工艺窗口",
                "mode": "section",
                "chapter_number": 2,
                "max_chars": 20,
                "part": 1
            }),
        )
        .expect("read literature section");

        assert_eq!(result["success"], json!(true));
        assert_eq!(result["answer_type"], json!("document_section"));
        assert_eq!(result["document"], json!("GB-T 1591.pdf"));
        assert!(result["content"]
            .as_str()
            .unwrap_or_default()
            .contains("Q355B"));
        assert_eq!(result["part"], json!(1));
        assert_eq!(result["has_more"], json!(true));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn compute_argument_normalization_accepts_params_and_snake_case() {
        let arguments = normalize_compute_arguments(json!({
            "query": "预测",
            "params": {
                "dataset_id": "dataset-1",
                "training_task_id": "training-1",
                "feature_values": [125.0, 0.2]
            }
        }));

        assert_eq!(arguments["datasetId"], json!("dataset-1"));
        assert_eq!(arguments["trainingTaskId"], json!("training-1"));
        assert_eq!(arguments["featureValues"], json!([125.0, 0.2]));
    }

    #[test]
    fn web_style_top_k_is_treated_as_limit() {
        assert_eq!(limit_arg(&json!({"top_k": 7}), 12), 7);
        assert_eq!(limit_arg(&json!({"top_k": 99}), 12), 50);
    }

    #[test]
    fn coil_targets_accept_web_filter_aliases() {
        assert_eq!(
            coil_match_targets(&json!({
                "target_yield": 460.0,
                "target_tensile": 550.0,
                "target_elongation": 21.0
            })),
            vec![
                ("yield_strength", 460.0),
                ("tensile_strength", 550.0),
                ("elongation", 21.0),
            ]
        );
    }

    #[test]
    fn web_compute_shapes_return_local_setup_prompt_instead_of_schema_errors() {
        let gateway = DesktopSteelAgentGateway::new(
            std::env::temp_dir().join("bloomery-unused-agent-gateway.sqlite3"),
            "local",
        );

        let prediction = execute(
            &gateway,
            "predict_performance",
            json!({"params": {"C": 0.12}}),
        )
        .expect("web prediction shape returns structured guidance");
        assert_eq!(prediction["success"], json!(false));
        assert_eq!(prediction["requires_user_action"], json!(true));

        let training = execute(
            &gateway,
            "start_training",
            json!({"model_version": "agent_v1"}),
        )
        .expect("web training shape returns structured guidance");
        assert_eq!(training["success"], json!(false));
        assert_eq!(training["requires_user_action"], json!(true));
    }

    #[test]
    fn query_production_data_searches_preview_rows() {
        let path = std::env::temp_dir().join(format!(
            "bloomery-steel-agent-gateway-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let (mut connection, _) = crate::storage::database::open(&path).expect("open migrated db");
        steel_repository::save_preview(
            &mut connection,
            "local",
            "F:/steel.csv",
            "sha256",
            &DatasetPreview {
                source_name: "steel.csv".to_string(),
                format: "csv".to_string(),
                sheets: Vec::new(),
                selected_sheet: String::new(),
                row_count: 1,
                column_count: 2,
                truncated: false,
                columns: vec![
                    DatasetColumnPreview {
                        name: "heat_id".to_string(),
                        duplicate: false,
                        inferred_type: "text".to_string(),
                        non_empty_count: 1,
                        missing_count: 0,
                        invalid_count: 0,
                        min: None,
                        max: None,
                    },
                    DatasetColumnPreview {
                        name: "grade".to_string(),
                        duplicate: false,
                        inferred_type: "text".to_string(),
                        non_empty_count: 1,
                        missing_count: 0,
                        invalid_count: 0,
                        min: None,
                        max: None,
                    },
                ],
                sample_rows: vec![vec!["H-01".to_string(), "Q355B".to_string()]],
                warnings: Vec::new(),
            },
            &[],
        )
        .expect("save dataset");
        drop(connection);
        let gateway = DesktopSteelAgentGateway::new(path.clone(), "local");

        let result = execute(&gateway, "query_production_data", json!({"query": "Q355B"}))
            .expect("query production data");

        assert_eq!(result["success"], json!(true));
        assert_eq!(result["datasets"][0]["sample_rows"][0][1], json!("Q355B"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn query_production_data_searches_source_rows_beyond_preview() {
        let database = std::env::temp_dir().join(format!(
            "bloomery-steel-agent-gateway-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let source =
            std::env::temp_dir().join(format!("bloomery-source-{}.csv", uuid::Uuid::new_v4()));
        let mut csv = String::from("heat_id,grade\n");
        for index in 1..=25 {
            let grade = if index == 25 { "Q690D" } else { "Q355B" };
            csv.push_str(&format!("H-{index:02},{grade}\n"));
        }
        std::fs::write(&source, csv).expect("write dataset source");
        let preview = preview_dataset(&DatasetPreviewRequest {
            source_path: source.to_string_lossy().into_owned(),
            sheet: None,
        })
        .expect("preview source");
        assert!(
            preview
                .sample_rows
                .iter()
                .all(|row| row.get(1).is_none_or(|value| value != "Q690D")),
            "fixture must place the match outside the preview rows"
        );

        let (mut connection, _) =
            crate::storage::database::open(&database).expect("open migrated db");
        steel_repository::save_preview(
            &mut connection,
            "local",
            &source.to_string_lossy(),
            "sha256",
            &preview,
            &[],
        )
        .expect("save dataset");
        drop(connection);
        let gateway = DesktopSteelAgentGateway::new(database.clone(), "local");

        let result = execute(&gateway, "query_production_data", json!({"query": "Q690D"}))
            .expect("query production data");

        assert_eq!(result["success"], json!(true));
        assert_eq!(
            result["datasets"][0]["sample_row_source"],
            json!("source_file")
        );
        assert_eq!(result["datasets"][0]["sample_rows"][0][1], json!("Q690D"));

        let _ = std::fs::remove_file(database);
        let _ = std::fs::remove_file(source);
    }

    #[test]
    fn match_coil_uses_mapped_preview_performance_columns() {
        let path = std::env::temp_dir().join(format!(
            "bloomery-steel-agent-gateway-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let (mut connection, _) = crate::storage::database::open(&path).expect("open migrated db");
        steel_repository::save_preview(
            &mut connection,
            "local",
            "F:/coils.csv",
            "sha256",
            &DatasetPreview {
                source_name: "coils.csv".to_string(),
                format: "csv".to_string(),
                sheets: Vec::new(),
                selected_sheet: String::new(),
                row_count: 2,
                column_count: 2,
                truncated: false,
                columns: vec![
                    DatasetColumnPreview {
                        name: "coil_id".to_string(),
                        duplicate: false,
                        inferred_type: "text".to_string(),
                        non_empty_count: 2,
                        missing_count: 0,
                        invalid_count: 0,
                        min: None,
                        max: None,
                    },
                    DatasetColumnPreview {
                        name: "YS".to_string(),
                        duplicate: false,
                        inferred_type: "number".to_string(),
                        non_empty_count: 2,
                        missing_count: 0,
                        invalid_count: 0,
                        min: Some(355.0),
                        max: Some(420.0),
                    },
                ],
                sample_rows: vec![
                    vec!["C-01".to_string(), "355".to_string()],
                    vec!["C-02".to_string(), "420".to_string()],
                ],
                warnings: Vec::new(),
            },
            &[steel_repository::DatasetColumnMapping {
                ordinal: 1,
                canonical_field: Some("yield_strength".to_string()),
                unit: Some("MPa".to_string()),
            }],
        )
        .expect("save dataset");
        drop(connection);
        let gateway = DesktopSteelAgentGateway::new(path.clone(), "local");

        let result =
            execute(&gateway, "match_coil", json!({"yield_strength": 355.0})).expect("match coil");

        assert_eq!(result["success"], json!(true));
        assert_eq!(result["matches"][0]["row"][0], json!("C-01"));
        assert_eq!(
            result["matches"][0]["values"]["yield_strength"],
            json!(355.0)
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn match_coil_searches_source_rows_beyond_preview() {
        let database = std::env::temp_dir().join(format!(
            "bloomery-steel-agent-gateway-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let source =
            std::env::temp_dir().join(format!("bloomery-coils-{}.csv", uuid::Uuid::new_v4()));
        let mut csv = String::from("coil_id,yield_strength\n");
        for index in 1..=25 {
            let strength = if index == 25 { 690 } else { 355 };
            csv.push_str(&format!("C-{index:02},{strength}\n"));
        }
        std::fs::write(&source, csv).expect("write coil source");
        let preview = preview_dataset(&DatasetPreviewRequest {
            source_path: source.to_string_lossy().into_owned(),
            sheet: None,
        })
        .expect("preview coils");
        assert!(
            preview
                .sample_rows
                .iter()
                .all(|row| row.get(1).is_none_or(|value| value != "690")),
            "fixture must place the match outside the preview rows"
        );

        let (mut connection, _) =
            crate::storage::database::open(&database).expect("open migrated db");
        steel_repository::save_preview(
            &mut connection,
            "local",
            &source.to_string_lossy(),
            "sha256",
            &preview,
            &[steel_repository::DatasetColumnMapping {
                ordinal: 1,
                canonical_field: Some("yield_strength".to_string()),
                unit: Some("MPa".to_string()),
            }],
        )
        .expect("save dataset");
        drop(connection);
        let gateway = DesktopSteelAgentGateway::new(database.clone(), "local");

        let result =
            execute(&gateway, "match_coil", json!({"yield_strength": 690.0})).expect("match coil");

        assert_eq!(result["success"], json!(true));
        assert_eq!(result["matches"][0]["row_source"], json!("source_file"));
        assert_eq!(result["matches"][0]["row"][0], json!("C-25"));

        let _ = std::fs::remove_file(database);
        let _ = std::fs::remove_file(source);
    }

    #[test]
    fn query_production_data_accepts_web_style_text_and_range_filters() {
        let database = std::env::temp_dir().join(format!(
            "bloomery-steel-agent-gateway-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let source =
            std::env::temp_dir().join(format!("bloomery-filter-{}.csv", uuid::Uuid::new_v4()));
        std::fs::write(
            &source,
            "heat_id,grade,yield_strength\nH-01,Q355B,350\nH-02,Q355B,420\nH-03,Q235B,420\n",
        )
        .expect("write production source");
        let preview = preview_dataset(&DatasetPreviewRequest {
            source_path: source.to_string_lossy().into_owned(),
            sheet: None,
        })
        .expect("preview source");
        let (mut connection, _) =
            crate::storage::database::open(&database).expect("open migrated db");
        steel_repository::save_preview(
            &mut connection,
            "local",
            &source.to_string_lossy(),
            "sha256",
            &preview,
            &[
                steel_repository::DatasetColumnMapping {
                    ordinal: 1,
                    canonical_field: Some("steel_grade".to_string()),
                    unit: None,
                },
                steel_repository::DatasetColumnMapping {
                    ordinal: 2,
                    canonical_field: Some("yield_strength".to_string()),
                    unit: Some("MPa".to_string()),
                },
            ],
        )
        .expect("save dataset");
        drop(connection);
        let gateway = DesktopSteelAgentGateway::new(database.clone(), "local");

        let result = execute(
            &gateway,
            "query_production_data",
            json!({
                "steel_grade": "Q355B",
                "yield_rp02_min": 400.0,
                "yield_rp02_max": 430.0
            }),
        )
        .expect("query production data");

        assert_eq!(result["success"], json!(true));
        assert_eq!(
            result["datasets"][0]["sample_rows"],
            json!([["H-02", "Q355B", "420"]])
        );
        assert_eq!(
            result["records"][0]["dataset_id"],
            result["datasets"][0]["id"]
        );
        assert_eq!(result["records"][0]["row"], json!(["H-02", "Q355B", "420"]));
        assert_eq!(
            result["records"][0]["values"]["steel_grade"],
            json!("Q355B")
        );
        assert_eq!(
            result["records"][0]["values"]["yield_strength"],
            json!(420.0)
        );

        let _ = std::fs::remove_file(database);
        let _ = std::fs::remove_file(source);
    }
}
