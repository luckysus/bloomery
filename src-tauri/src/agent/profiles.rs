use crate::agent::desktop::AgentPreferences;
use crate::providers::profiles::ProviderCapability;
use crate::storage::repositories::{provider_profiles, settings};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use uuid::Uuid;

pub const AGENT_PROFILES_KEY: &str = "agent.profiles";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentPermissionRestrictions {
    pub allow_file_access: bool,
    pub allow_shell: bool,
    pub allow_network: bool,
    pub allow_database: bool,
    pub allow_mcp: bool,
    pub confirm_dangerous: bool,
}

impl Default for AgentPermissionRestrictions {
    fn default() -> Self {
        Self {
            allow_file_access: true,
            allow_shell: false,
            allow_network: true,
            allow_database: true,
            allow_mcp: true,
            confirm_dangerous: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentProfileLimits {
    pub max_turns: usize,
    pub max_tool_calls: usize,
    pub context_budget: usize,
    pub retries: usize,
    #[serde(default = "default_recovery_retries")]
    pub recovery_retries: usize,
    #[serde(default = "default_run_timeout_seconds")]
    pub run_timeout_seconds: u64,
}

fn default_recovery_retries() -> usize {
    2
}
fn default_run_timeout_seconds() -> u64 {
    1_800
}

impl Default for AgentProfileLimits {
    fn default() -> Self {
        Self {
            max_turns: 20,
            max_tool_calls: 64,
            context_budget: 32_768,
            retries: 2,
            recovery_retries: default_recovery_retries(),
            run_timeout_seconds: default_run_timeout_seconds(),
        }
    }
}

impl AgentProfileLimits {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=100).contains(&self.max_turns)
            || !(1..=1_000).contains(&self.max_tool_calls)
            || !(1_024..=262_144).contains(&self.context_budget)
            || self.retries > 10
            || self.recovery_retries > 10
            || !(30..=86_400).contains(&self.run_timeout_seconds)
        {
            return Err("Agent execution limits are outside the supported range".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub status: String,
    pub preset: bool,
    pub system_prompt: String,
    pub provider_id: Option<String>,
    /// Exact tool IDs; an empty allowlist grants no tools.
    pub tool_ids: Vec<String>,
    pub permission_restrictions: AgentPermissionRestrictions,
    pub limits: AgentProfileLimits,
}

impl AgentProfile {
    pub fn validate(mut self) -> Result<Self, String> {
        self.id = self.id.trim().to_string();
        if self.id.is_empty()
            || self.id.len() > 80
            || !self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(
                "Agent ID must contain 1–80 letters, digits, underscores or hyphens".to_string(),
            );
        }
        self.name = self.name.trim().to_string();
        self.description = self.description.trim().to_string();
        self.system_prompt = self.system_prompt.trim().to_string();
        if self.name.is_empty() || self.name.chars().count() > 80 {
            return Err("Agent name must contain 1–80 characters".to_string());
        }
        if self.description.chars().count() > 2_000 {
            return Err("Agent description is too long".to_string());
        }
        if self.system_prompt.is_empty() || self.system_prompt.chars().count() > 16_000 {
            return Err("System Prompt must contain 1–16000 characters".to_string());
        }
        if let Some(id) = &self.provider_id {
            self.provider_id = Some(
                Uuid::parse_str(id.trim())
                    .map_err(|_| "Provider ID must be a UUID".to_string())?
                    .to_string(),
            );
        }
        if self.tool_ids.len() > 256 {
            return Err("Agent tool allowlist is too large".to_string());
        }
        self.tool_ids = self
            .tool_ids
            .into_iter()
            .map(|id| id.trim().to_string())
            .collect();
        for id in &self.tool_ids {
            crate::tools::ToolId::new(id.clone()).map_err(|_| format!("Invalid tool ID: {id}"))?;
        }
        self.tool_ids.sort();
        self.tool_ids.dedup();
        self.limits.validate()?;
        self.preset = preset_ids().contains(&self.id.as_str());
        self.status = if self.enabled {
            "available"
        } else {
            "disabled"
        }
        .to_string();
        Ok(self)
    }

    /// Profile permissions and budgets only narrow the parent/global boundary.
    pub fn restrict_preferences(&self, preferences: &mut AgentPreferences) {
        let restrictions = &self.permission_restrictions;
        preferences.allow_file_access &= restrictions.allow_file_access;
        preferences.allow_shell &= restrictions.allow_shell;
        preferences.allow_network &= restrictions.allow_network;
        preferences.allow_database &= restrictions.allow_database;
        preferences.allow_mcp &= restrictions.allow_mcp;
        preferences.confirm_dangerous |= restrictions.confirm_dangerous;
        preferences.max_turns = preferences.max_turns.min(self.limits.max_turns);
        preferences.max_tool_calls = preferences.max_tool_calls.min(self.limits.max_tool_calls);
        preferences.context_budget = preferences.context_budget.min(self.limits.context_budget);
        preferences.retries = preferences.retries.min(self.limits.retries);
        preferences.recovery_retries = preferences
            .recovery_retries
            .min(self.limits.recovery_retries);
        preferences.run_timeout_seconds = preferences
            .run_timeout_seconds
            .min(self.limits.run_timeout_seconds);
    }
}

pub fn preset_ids() -> [&'static str; 9] {
    [
        "master",
        "knowledge",
        "literature",
        "data",
        "material",
        "prediction",
        "optimization",
        "experiment",
        "report",
    ]
}

/// These IDs are the registrations exposed by the local runtime, not UI categories.
pub fn builtin_tool_capabilities() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (
            "builtin.read_file",
            "读取文件",
            "读取 Agent 工作目录内的 UTF-8 文本文件",
        ),
        (
            "builtin.list_directory",
            "列出目录",
            "查看 Agent 工作目录内的文件和目录",
        ),
        (
            "builtin.write_file",
            "写入文件",
            "在 Agent 工作目录内创建或修改 UTF-8 文件",
        ),
        (
            "builtin.powershell",
            "PowerShell",
            "在 Agent 工作目录内执行受权限限制的命令",
        ),
        (
            "agent.task",
            "委派专家任务",
            "在父任务权限和预算内执行独立子任务",
        ),
        ("agent.todo_write", "任务计划", "记录和更新执行步骤"),
        ("agent.load_skill", "加载 Skill", "读取已启用的专业工作流程"),
        ("steel.knowledge_search", "知识检索", "检索已接入的知识库"),
        ("steel.search_literature", "文献检索", "检索文献和证据"),
        (
            "steel.read_literature_section",
            "文献章节",
            "读取指定文献章节",
        ),
        (
            "steel.query_production_data",
            "生产数据查询",
            "查询已配置的生产数据",
        ),
        (
            "steel.query_composition_standard",
            "成分标准",
            "查询材料成分标准",
        ),
        ("steel.query_process_standard", "工艺标准", "查询工艺标准"),
        (
            "steel.ask_llm_with_context",
            "上下文分析",
            "根据已加载内容分析问题",
        ),
        ("steel.match_coil", "钢卷匹配", "匹配满足材料约束的钢卷"),
        ("steel.get_model_status", "模型状态", "查询已训练模型的状态"),
        (
            "steel.predict_performance",
            "性能预测",
            "调用已配置模型预测性能",
        ),
        ("steel.optimize_process", "工艺优化", "根据模型推荐工艺方案"),
        ("steel.start_training", "模型训练", "启动数据训练任务"),
        ("steel.process_literature", "文献处理", "导入并处理科研文献"),
        ("steel.export_data", "导出数据", "导出分析结果到文件"),
        ("steel.remember_memory", "保存记忆", "保存可复用的研究记录"),
        ("steel.read_memory", "读取记忆", "读取研究记忆"),
        ("steel.search_memory", "检索记忆", "检索研究记忆"),
        ("steel.list_memory", "列出记忆", "查看研究记忆目录"),
        ("steel.forget_memory", "删除记忆", "删除指定研究记忆"),
        (
            "steel.carbon_equivalent",
            "碳当量",
            "计算 IIW 或 Pcm 碳当量",
        ),
        (
            "steel.optimize_constrained",
            "约束优化",
            "在明确约束下优化工艺",
        ),
        (
            "steel.optimization_status",
            "优化状态",
            "查询约束优化任务进度",
        ),
    ]
}

pub fn presets() -> Vec<AgentProfile> {
    let all_tools = builtin_tool_capabilities()
        .into_iter()
        .map(|(id, _, _)| id.to_string())
        .collect::<Vec<_>>();
    let research_tools = [
        "builtin.read_file",
        "builtin.list_directory",
        "agent.todo_write",
        "agent.load_skill",
        "steel.knowledge_search",
        "steel.search_literature",
        "steel.read_literature_section",
        "steel.read_memory",
        "steel.search_memory",
        "steel.list_memory",
    ];
    [
        ("master", "Master Agent", "任务理解、规划和结果整合", "先理解用户目标和约束，再规划任务；按专业职责委派专家，核对证据、权限与预算，整合最终结果。", Vec::new()),
        ("knowledge", "Knowledge Agent", "知识检索和证据引用", "检索知识库和标准，优先给出可核验的原文与引用，明确来源、适用条件及证据缺口。", vec!["steel.query_composition_standard", "steel.query_process_standard"]),
        ("literature", "Literature Agent", "文献检索、总结和综述", "检索和精读科研文献，区分论文结果与个人推断，比较方法和局限；不得编造文献、DOI或引用。", vec!["steel.process_literature"]),
        ("data", "Data Agent", "数据清洗、统计和可视化", "检查数据质量、单位、缺失值和异常，说明统计假设与不确定性，保留可复现的分析依据。", vec!["builtin.write_file", "steel.query_production_data", "steel.export_data", "steel.start_training"]),
        ("material", "Material Agent", "成分、组织和性能关联", "从成分、组织、加工历史和性能关系分析材料问题，核对标准适用范围，给出有证据支持的机制解释。", vec!["steel.query_composition_standard", "steel.query_process_standard", "steel.carbon_equivalent", "steel.match_coil"]),
        ("prediction", "Prediction Agent", "性能预测和模型解释", "使用已有模型进行性能预测，检查输入单位、模型适用范围及验证证据，说明误差和外推风险。", vec!["steel.get_model_status", "steel.predict_performance", "steel.start_training"]),
        ("optimization", "Optimization Agent", "工艺约束和候选方案", "先明确目标、可操作变量和硬约束，再执行工艺优化；报告候选方案、约束满足情况及验证建议。", vec!["steel.query_process_standard", "steel.optimize_process", "steel.optimize_constrained", "steel.optimization_status", "steel.predict_performance"]),
        ("experiment", "Experiment Agent", "DOE 和下一实验推荐", "根据研究假设和已有数据设计实验，明确因素、水平、对照、重复、样本量依据以及下一步验证标准。", vec!["steel.query_production_data", "steel.query_process_standard", "steel.export_data", "steel.predict_performance"]),
        ("report", "Report Agent", "科研报告和引用整理", "依据已有证据整理科研报告，区分事实、推断和待验证内容，保留引用和关键限制，避免夸大结论。", vec!["builtin.write_file", "steel.export_data"]),
    ].into_iter().map(|(id, name, description, prompt, extra)| {
        let tool_ids = if id == "master" { all_tools.clone() } else { research_tools.iter().chain(extra.iter()).map(|id| (*id).to_string()).collect() };
        AgentProfile {
            id: id.to_string(), name: name.to_string(), description: description.to_string(), enabled: true,
            status: "available".to_string(), preset: true,
            system_prompt: format!("你是 Suna 的钢铁材料研发 {name}。回答必须区分事实、推断和待验证内容。{prompt}"),
            provider_id: None, tool_ids,
            permission_restrictions: AgentPermissionRestrictions::default(), limits: AgentProfileLimits::default(),
        }.validate().expect("valid builtin Agent profile")
    }).collect()
}

pub fn list(connection: &Connection, workspace_id: &str) -> Result<Vec<AgentProfile>, String> {
    let raw = settings::get(connection, workspace_id, AGENT_PROFILES_KEY)?;
    let saved: Vec<AgentProfile> = raw
        .map(|raw| {
            serde_json::from_str(&raw)
                .map_err(|error| format!("Agent profiles are invalid: {error}"))
        })
        .transpose()?
        .unwrap_or_default();
    if saved.len() > 100 {
        return Err("Too many Agent profiles".to_string());
    }
    let mut seen = BTreeSet::new();
    let saved = saved
        .into_iter()
        .map(|profile| {
            let profile = profile.validate()?;
            if !seen.insert(profile.id.clone()) {
                return Err("Duplicate Agent profile ID".to_string());
            }
            Ok(profile)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut profiles = presets();
    for profile in saved {
        if let Some(existing) = profiles.iter_mut().find(|item| item.id == profile.id) {
            *existing = profile;
        } else {
            profiles.push(profile);
        }
    }
    Ok(profiles)
}

pub fn get(
    connection: &Connection,
    workspace_id: &str,
    id: &str,
) -> Result<Option<AgentProfile>, String> {
    Ok(list(connection, workspace_id)?
        .into_iter()
        .find(|profile| profile.id == id))
}

pub fn save(
    connection: &mut Connection,
    workspace_id: &str,
    profile: AgentProfile,
) -> Result<AgentProfile, String> {
    let profile = profile.validate()?;
    if let Some(id) = profile.provider_id.as_ref().filter(|_| profile.enabled) {
        let provider = provider_profiles::get(
            connection,
            workspace_id,
            Uuid::parse_str(id).map_err(|error| error.to_string())?,
        )?
        .ok_or_else(|| "The selected Provider no longer exists".to_string())?;
        if !provider.enabled || !provider.kind.supports(ProviderCapability::Chat) {
            return Err("The selected Provider must be enabled and support chat".to_string());
        }
    }
    let mut profiles = list(connection, workspace_id)?;
    if let Some(existing) = profiles.iter_mut().find(|item| item.id == profile.id) {
        *existing = profile.clone();
    } else if profiles.len() < 100 {
        profiles.push(profile.clone());
    } else {
        return Err("Too many Agent profiles".to_string());
    }
    persist(connection, workspace_id, &profiles)?;
    Ok(profile)
}

pub fn reset(
    connection: &mut Connection,
    workspace_id: &str,
    id: &str,
) -> Result<AgentProfile, String> {
    let profile = presets()
        .into_iter()
        .find(|profile| profile.id == id)
        .ok_or_else(|| "Only preset Agent profiles can be reset".to_string())?;
    save(connection, workspace_id, profile)
}

pub fn delete(connection: &mut Connection, workspace_id: &str, id: &str) -> Result<(), String> {
    if preset_ids().contains(&id) {
        return Err("Preset Agents cannot be deleted; disable them instead".to_string());
    }
    let mut profiles = list(connection, workspace_id)?;
    if !profiles.iter().any(|profile| profile.id == id) {
        return Err("Agent profile not found".to_string());
    }
    profiles.retain(|profile| profile.id != id);
    persist(connection, workspace_id, &profiles)
}

fn persist(
    connection: &mut Connection,
    workspace_id: &str,
    profiles: &[AgentProfile],
) -> Result<(), String> {
    let raw = serde_json::to_string(profiles).map_err(|error| error.to_string())?;
    settings::set(connection, workspace_id, AGENT_PROFILES_KEY, &raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_persist_independently_and_preset_reset_is_scoped() {
        let mut connection = Connection::open_in_memory().unwrap();
        crate::storage::migrations::migrate(&mut connection).unwrap();
        let original = presets();
        assert_eq!(original.len(), 9);
        assert_ne!(original[1].system_prompt, original[2].system_prompt);
        let mut profile = original[1].clone();
        profile.enabled = false;
        profile.system_prompt = "Only return cited evidence".to_string();
        profile.tool_ids = vec!["steel.knowledge_search".to_string()];
        save(&mut connection, "local", profile.clone()).unwrap();
        assert_eq!(
            get(&connection, "local", "knowledge")
                .unwrap()
                .unwrap()
                .system_prompt,
            profile.system_prompt
        );
        assert_eq!(
            get(&connection, "other", "knowledge").unwrap().unwrap(),
            original[1]
        );
        assert!(delete(&mut connection, "local", "knowledge").is_err());
        assert_eq!(
            reset(&mut connection, "local", "knowledge").unwrap(),
            original[1]
        );
        profile.id = "custom-evidence".to_string();
        save(&mut connection, "local", profile).unwrap();
        delete(&mut connection, "local", "custom-evidence").unwrap();
        assert_eq!(list(&connection, "local").unwrap().len(), 9);
        let mut unavailable = original[2].clone();
        unavailable.provider_id = Some(Uuid::new_v4().to_string());
        assert!(save(&mut connection, "local", unavailable.clone()).is_err());
        unavailable.enabled = false;
        assert!(save(&mut connection, "local", unavailable).is_ok());
    }

    #[test]
    fn restrictions_never_widen_parent_permissions_or_budgets() {
        let mut profile = presets().remove(0);
        let mut parent = AgentPreferences::default();
        parent.allow_file_access = false;
        parent.max_turns = 3;
        profile.permission_restrictions.allow_shell = true;
        profile.permission_restrictions.confirm_dangerous = false;
        profile.permission_restrictions.allow_network = false;
        profile.restrict_preferences(&mut parent);
        assert!(!parent.allow_file_access && !parent.allow_shell && !parent.allow_network);
        assert!(parent.confirm_dangerous);
        assert_eq!(parent.max_turns, 3);
        profile.tool_ids = vec!["*".to_string()];
        assert!(profile.validate().is_err());
    }
}
