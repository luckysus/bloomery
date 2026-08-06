use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomainManifest {
    pub id: String,
    pub version: String,
    pub compatibility: AppCompatibility,
    pub author: String,
    pub license: String,
    pub prompts: PromptSpec,
    #[serde(default)]
    pub terminology: BTreeMap<String, String>,
    pub retrieval: RetrievalPolicy,
    #[serde(default)]
    pub builtin_tool_allowlist: Vec<String>,
    #[serde(default)]
    pub mcp_recommendations: Vec<McpRecommendation>,
    #[serde(default)]
    pub data_mappings: Vec<DataMapping>,
    #[serde(default)]
    pub evaluations: Vec<EvaluationSpec>,
    #[serde(default)]
    pub assets: Vec<AssetSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppCompatibility {
    pub min_app_version: String,
    pub max_app_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptSpec {
    pub system: String,
    pub workflow: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalPolicy {
    #[serde(default)]
    pub required_tags: Vec<String>,
    #[serde(default = "default_true")]
    pub citation_required: bool,
    #[serde(default = "default_max_evidence_items")]
    pub max_evidence_items: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpRecommendation {
    pub id: String,
    pub transport: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataMapping {
    pub dataset: String,
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
    #[serde(default)]
    pub units: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationSpec {
    pub id: String,
    pub kind: String,
    pub dataset: String,
    pub expected_behavior: String,
    pub threshold: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetSpec {
    pub path: String,
    pub kind: String,
    pub sha256: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_max_evidence_items() -> usize {
    12
}
