use bloomery::agent::context::{
    budget_context, estimate_tokens, ContextBudgetError, ContextItem, ContextSource,
    TruncationRecord, DEFAULT_MODEL_LIMIT,
};

fn item(id: &str, source: ContextSource, content: &str) -> ContextItem {
    ContextItem::new(id, source, content)
}

fn recent(id: &str, newest_first_rank: usize, content: &str) -> ContextItem {
    item(id, ContextSource::RecentTurn { newest_first_rank }, content)
}

fn estimate_for(items: &[ContextItem], ids: &[&str]) -> usize {
    items
        .iter()
        .filter(|item| ids.contains(&item.id.as_str()))
        .map(|item| estimate_tokens(&item.content))
        .sum()
}

#[test]
fn token_estimator_distinguishes_latin_and_cjk() {
    assert_eq!(estimate_tokens("steel"), 2);
    assert_eq!(estimate_tokens("steelsteel"), 3);
    assert_eq!(estimate_tokens("钢铁钢铁"), 4);
    assert!(estimate_tokens("钢铁钢铁") > estimate_tokens("steelsteel"));
}

#[test]
fn token_estimator_counts_whitespace_and_mixed_structured_text() {
    assert_eq!(estimate_tokens(" \n\t"), 3);
    assert_eq!(estimate_tokens("a 钢\n{}"), 6);
}

#[test]
fn required_rules_and_current_request_are_preserved_before_optional_context() {
    let items = vec![
        item("domain", ContextSource::Domain, "domain rule"),
        item("security", ContextSource::Security, "security rule"),
        item("permission", ContextSource::Permission, "permission rule"),
        item("system", ContextSource::System, "system rule"),
        item("request", ContextSource::CurrentRequest, "current request"),
        item("evidence", ContextSource::ToolEvidence, "tool evidence"),
        item("memory", ContextSource::ExplicitMemory, "explicit memory"),
        item("summary", ContextSource::HistoricalSummary, "old summary"),
    ];
    let required_tokens = estimate_for(
        &items,
        &["domain", "security", "permission", "system", "request"],
    );

    let report = budget_context(&items, Some(required_tokens), 0).expect("required context fits");

    assert_eq!(
        report.included_ids,
        vec!["domain", "security", "permission", "system", "request"]
    );
    assert_eq!(report.omitted_ids, vec!["evidence", "memory", "summary"]);
    assert!(report.truncations.is_empty());
    for required in ["domain", "security", "permission", "system", "request"] {
        let included = report
            .included_items
            .iter()
            .find(|item| item.id == required)
            .expect("required item is included");
        let original = items.iter().find(|item| item.id == required).unwrap();
        assert_eq!(included.content, original.content);
    }
}

#[test]
fn required_content_overflow_returns_a_typed_error() {
    let items = vec![
        item("security", ContextSource::Security, "permission rule"),
        item("request", ContextSource::CurrentRequest, "current request"),
    ];
    let required_tokens = estimate_for(&items, &["security", "request"]);
    let model_limit = required_tokens - 1;

    let error =
        budget_context(&items, Some(model_limit), 0).expect_err("required content overflows");

    assert_eq!(
        error,
        ContextBudgetError::RequiredContentExceedsLimit {
            required_tokens,
            input_limit: model_limit,
            model_limit,
        }
    );
}

#[test]
fn current_request_is_required() {
    let items = vec![item("system", ContextSource::System, "system rule")];

    assert_eq!(
        budget_context(&items, Some(100), 0).expect_err("request is missing"),
        ContextBudgetError::MissingCurrentRequest
    );
}

#[test]
fn current_request_must_be_unique_and_nonempty() {
    let empty = vec![item("request", ContextSource::CurrentRequest, "  \n")];
    assert_eq!(
        budget_context(&empty, Some(100), 0).expect_err("request is empty"),
        ContextBudgetError::EmptyCurrentRequest
    );

    let duplicate = vec![
        item("request-1", ContextSource::CurrentRequest, "first"),
        item("request-2", ContextSource::CurrentRequest, "second"),
    ];
    assert_eq!(
        budget_context(&duplicate, Some(100), 0).expect_err("request is duplicated"),
        ContextBudgetError::MultipleCurrentRequests { count: 2 }
    );
}

#[test]
fn priority_selection_is_deterministic_across_evidence_memory_and_summary() {
    let items = vec![
        item("domain", ContextSource::Domain, "domain rule"),
        item("security", ContextSource::Security, "permission rule"),
        item("system", ContextSource::System, "system rule"),
        item("request", ContextSource::CurrentRequest, "current request"),
        recent("oldest", 2, "old conversation message is too large"),
        recent("middle", 1, "middle conversation message is too large"),
        recent("newest", 0, "new message"),
        item("evidence-1", ContextSource::ToolEvidence, "evidence"),
        item("evidence-2", ContextSource::ToolEvidence, "second evidence"),
        item("memory", ContextSource::ExplicitMemory, "memory"),
        item("summary", ContextSource::HistoricalSummary, "summary"),
    ];
    let required_tokens = estimate_for(&items, &["domain", "security", "system", "request"]);
    let input_limit = required_tokens + estimate_for(&items, &["newest", "evidence-1"]);

    let first = budget_context(&items, Some(input_limit), 0).expect("budget fits");
    let second = budget_context(&items, Some(input_limit), 0).expect("same budget fits");

    assert_eq!(first, second);
    assert_eq!(
        first.included_ids,
        vec![
            "domain",
            "security",
            "system",
            "request",
            "newest",
            "evidence-1"
        ]
    );
    assert_eq!(
        first.omitted_ids,
        vec!["middle", "oldest", "evidence-2", "memory", "summary"]
    );
    assert_eq!(first.estimated_included_tokens, first.input_limit);
}

#[test]
fn recent_turns_are_returned_newest_first() {
    let items = vec![
        item("request", ContextSource::CurrentRequest, "request"),
        recent("oldest", 2, "oldest"),
        recent("newest", 0, "newest"),
        recent("middle", 1, "middle"),
    ];
    let input_limit = items
        .iter()
        .map(|item| estimate_tokens(&item.content))
        .sum();

    let report = budget_context(&items, Some(input_limit), 0).expect("all messages fit");

    assert_eq!(
        report.included_ids,
        vec!["request", "newest", "middle", "oldest"]
    );
}

#[test]
fn recent_turns_are_atomic_and_contiguous() {
    let items = vec![
        item("request", ContextSource::CurrentRequest, "request"),
        recent("newest", 0, "newest"),
        recent("middle", 1, &"middle".repeat(20)),
        recent("oldest", 2, "oldest"),
    ];
    let input_limit = estimate_for(&items, &["request", "newest", "oldest"]);

    let report = budget_context(&items, Some(input_limit), 0).expect("newest turn fits");

    assert_eq!(report.included_ids, vec!["request", "newest"]);
    assert_eq!(report.omitted_ids, vec!["middle", "oldest"]);
}

#[test]
fn recent_turn_ranks_must_be_unique() {
    let items = vec![
        item("request", ContextSource::CurrentRequest, "request"),
        recent("turn-1", 0, "first"),
        recent("turn-2", 0, "second"),
    ];

    assert_eq!(
        budget_context(&items, Some(100), 0).expect_err("turn rank is duplicated"),
        ContextBudgetError::DuplicateRecentTurnRank {
            newest_first_rank: 0,
        }
    );
}

#[test]
fn optional_unicode_content_is_truncated_on_a_character_boundary_and_recorded() {
    let evidence_text = "钢铁锻造".repeat(4);
    let items = vec![
        item("request", ContextSource::CurrentRequest, "问"),
        item("evidence", ContextSource::ToolEvidence, &evidence_text),
        item("memory", ContextSource::ExplicitMemory, "memory"),
    ];
    let input_limit = estimate_tokens("问") + 3;

    let report = budget_context(&items, Some(input_limit), 0).expect("request and prefix fit");
    let included = report
        .included_items
        .iter()
        .find(|item| item.id == "evidence")
        .expect("evidence prefix is included");

    assert_eq!(included.content, "钢铁锻");
    assert_eq!(
        report.truncations,
        vec![TruncationRecord {
            id: "evidence".to_string(),
            original_tokens: estimate_tokens(&evidence_text),
            included_tokens: 3,
        }]
    );
    assert_eq!(report.omitted_ids, vec!["memory"]);
    assert!(report
        .item_token_estimates
        .iter()
        .any(|estimate| estimate.id == "evidence"
            && estimate.original_tokens > estimate.included_tokens));
}

#[test]
fn large_unicode_content_is_truncated_to_the_exact_token_limit() {
    let evidence_text = "钢".repeat(100_000);
    let items = vec![
        item("request", ContextSource::CurrentRequest, "问"),
        item("evidence", ContextSource::ToolEvidence, &evidence_text),
    ];
    let evidence_limit = 1_024;
    let input_limit = estimate_tokens("问") + evidence_limit;

    let report = budget_context(&items, Some(input_limit), 0).expect("evidence prefix fits");
    let included = report
        .included_items
        .iter()
        .find(|item| item.id == "evidence")
        .expect("evidence prefix is included");

    assert_eq!(included.content.chars().count(), evidence_limit);
    assert_eq!(estimate_tokens(&included.content), evidence_limit);
}

#[test]
fn provider_context_limit_reserves_output_and_defaults_deterministically() {
    let items = vec![item("request", ContextSource::CurrentRequest, "request")];

    let limited = budget_context(&items, Some(100), 30).expect("request fits");
    assert_eq!(limited.model_limit, 100);
    assert_eq!(limited.input_limit, 70);
    assert!(limited.estimated_included_tokens <= limited.input_limit);

    let defaulted = budget_context(&items, None, 100).expect("request fits default limit");
    assert_eq!(defaulted.model_limit, DEFAULT_MODEL_LIMIT);
    assert_eq!(defaulted.input_limit, DEFAULT_MODEL_LIMIT - 100);
}

#[test]
fn output_reservation_cannot_exceed_the_model_limit() {
    let items = vec![item("request", ContextSource::CurrentRequest, "request")];

    assert_eq!(
        budget_context(&items, Some(100), 101).expect_err("reservation is invalid"),
        ContextBudgetError::OutputReservationExceedsModelLimit {
            output_reservation: 101,
            model_limit: 100,
        }
    );
}
