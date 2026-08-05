use super::estimate_tokens;

pub const SUMMARY_TRIGGER_TOKENS: usize = 9_000;
pub const SUMMARY_KEEP_TAIL_TOKENS: usize = 3_200;
pub const SUMMARY_MIN_FOLD_TOKENS: usize = 1_200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SummaryMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SummaryPlan {
    pub older_messages: Vec<SummaryMessage>,
    pub covered_message_id: String,
    pub total_tokens: usize,
    pub folded_tokens: usize,
}

pub fn plan_summary(
    messages: Vec<SummaryMessage>,
    covered_message_id: Option<&str>,
) -> Result<Option<SummaryPlan>, String> {
    if messages.is_empty() {
        return Ok(None);
    }
    let total_tokens = estimate_summary_tokens(&messages);
    if let Some(anchor_id) = covered_message_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let index = messages
            .iter()
            .position(|message| message.id == anchor_id)
            .ok_or_else(|| "covered message not found".to_string())?;
        let older_messages = messages[..=index].to_vec();
        let folded_tokens = older_messages.iter().map(message_tokens).sum();
        return Ok(Some(SummaryPlan {
            older_messages,
            covered_message_id: anchor_id.to_string(),
            total_tokens: folded_tokens,
            folded_tokens,
        }));
    }
    if total_tokens < SUMMARY_TRIGGER_TOKENS {
        return Ok(None);
    }

    let mut tail_tokens = 0usize;
    let mut split_index = messages.len();
    for index in (0..messages.len()).rev() {
        let cost = message_tokens(&messages[index]);
        if tail_tokens >= SUMMARY_KEEP_TAIL_TOKENS && messages.len() - index >= 8 {
            split_index = index + 1;
            break;
        }
        tail_tokens = tail_tokens.saturating_add(cost);
        split_index = index;
    }
    let older_messages = messages[..split_index].to_vec();
    let folded_tokens = older_messages.iter().map(message_tokens).sum();
    let Some(covered_message_id) = older_messages.last().map(|message| message.id.clone()) else {
        return Ok(None);
    };
    if folded_tokens < SUMMARY_MIN_FOLD_TOKENS {
        return Ok(None);
    }
    Ok(Some(SummaryPlan {
        older_messages,
        covered_message_id,
        total_tokens,
        folded_tokens,
    }))
}

pub fn estimate_summary_tokens(messages: &[SummaryMessage]) -> usize {
    messages.iter().map(message_tokens).sum()
}

pub fn build_summary_prompt(
    plan: &SummaryPlan,
    existing_summary: Option<&str>,
) -> (String, Vec<String>) {
    let mut contexts = existing_summary
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|summary| vec![format!("[existing conversation summary]\n{summary}")])
        .unwrap_or_default();
    contexts.extend(plan.older_messages.iter().map(|item| {
        format!(
            "[{} id={} at={}]\n{}",
            item.role, item.id, item.created_at, item.content
        )
    }));
    let query = vec![
        "请把这些较早的桌面端智能体对话压缩成事实性摘要。".to_string(),
        "必须使用以下小标题；没有内容的小标题可以省略：".to_string(),
        "## 用户长期要求与硬约束".to_string(),
        "## 当前研究或生产目标".to_string(),
        "## 材料体系、钢种、成分与工艺参数".to_string(),
        "## 已讨论结论与关键假设".to_string(),
        "## 引用过的文献、数据来源或工具结果".to_string(),
        "## 已失败或被否定的方向".to_string(),
        "## 待解决问题与下一步".to_string(),
        "规则：保留具体数字、成分、温度、时间、钢种、文件名、任务 ID 和用户明确偏好；不要编造未知事实；使用短句和项目符号。".to_string(),
        format!(
            "本次折叠约 {} tokens，总历史约 {} tokens。",
            plan.folded_tokens, plan.total_tokens
        ),
    ]
    .join("\n");
    (query, contexts)
}

fn message_tokens(message: &SummaryMessage) -> usize {
    estimate_tokens(&message.role) + estimate_tokens(&message.content) + 4
}

pub fn messages_after_covered_id(
    mut messages: Vec<SummaryMessage>,
    covered_message_id: Option<&str>,
) -> Vec<SummaryMessage> {
    let Some(covered_message_id) = covered_message_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return messages;
    };
    if let Some(index) = messages
        .iter()
        .position(|message| message.id == covered_message_id)
    {
        messages.drain(..=index);
    }
    messages
}
