use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AgentProfileSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCapabilitySummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub source: String,
}

#[tauri::command]
pub fn list_agent_profiles() -> Vec<AgentProfileSummary> {
    [
        ("master", "Master Agent", "任务理解、规划和结果整合"),
        ("knowledge", "Knowledge Agent", "知识检索和证据引用"),
        ("literature", "Literature Agent", "文献检索、总结和综述"),
        ("data", "Data Agent", "数据清洗、统计和可视化"),
        ("material", "Material Agent", "成分、组织和性能关联"),
        ("prediction", "Prediction Agent", "性能预测和模型解释"),
        ("optimization", "Optimization Agent", "工艺约束和候选方案"),
        ("experiment", "Experiment Agent", "DOE 和下一实验推荐"),
        ("report", "Report Agent", "科研报告和引用整理"),
    ]
    .into_iter()
    .map(|(id, name, description)| AgentProfileSummary {
        id: id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        enabled: true,
        status: "available".to_string(),
    })
    .collect()
}

#[tauri::command]
pub fn list_tool_capabilities() -> Vec<ToolCapabilitySummary> {
    [
        (
            "filesystem",
            "文件系统",
            "文件读取、写入和导入",
            true,
            "builtin",
        ),
        (
            "knowledge_search",
            "知识检索",
            "全文、向量和混合检索",
            true,
            "builtin",
        ),
        (
            "data_analysis",
            "数据分析",
            "数据集统计与质量检查",
            true,
            "builtin",
        ),
        (
            "compute",
            "模型计算",
            "训练、预测和优化任务",
            true,
            "builtin",
        ),
        ("web_search", "网络搜索", "外部资料检索", false, "optional"),
        ("python", "Python", "脚本和科研计算", false, "optional"),
    ]
    .into_iter()
    .map(
        |(id, name, description, enabled, source)| ToolCapabilitySummary {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            enabled,
            source: source.to_string(),
        },
    )
    .collect()
}

#[cfg(test)]
mod tests {
    use super::{list_agent_profiles, list_tool_capabilities};

    #[test]
    fn exposes_the_documented_agent_roster() {
        let agents = list_agent_profiles();
        assert_eq!(agents.len(), 9);
        assert_eq!(agents[0].id, "master");
        assert!(agents.iter().all(|agent| agent.enabled));
    }

    #[test]
    fn separates_builtin_and_optional_tools() {
        let tools = list_tool_capabilities();
        assert!(tools
            .iter()
            .any(|tool| tool.id == "compute" && tool.enabled));
        assert!(tools
            .iter()
            .any(|tool| tool.id == "python" && !tool.enabled));
    }
}
