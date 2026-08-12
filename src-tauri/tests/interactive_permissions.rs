use bloomery::agent::desktop::permission_key_for;
use bloomery::agent::desktop::LocalAgentState;
use bloomery::agent::protocol::{PermissionDecision, PermissionRisk};
use bloomery::agent::runtime::{CancellationToken, PermissionRequest, PermissionResolver};
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use uuid::Uuid;

fn request(permission_id: Uuid) -> PermissionRequest {
    PermissionRequest {
        permission_id,
        tool_call_id: Uuid::new_v4(),
        tool_id: "builtin.write_file".to_string(),
        tool_name: "write_file".to_string(),
        risk: PermissionRisk::ConfirmationRequired,
        arguments: json!({"path": "draft.txt"}),
    }
}

#[tokio::test]
async fn interactive_permissions_wait_until_the_desktop_command_resolves_them() {
    let state = LocalAgentState::default();
    let resolver = state.permission_resolver();
    let permission_id = Uuid::new_v4();

    let task = tokio::spawn({
        let resolver = resolver.clone();
        async move {
            resolver
                .decide(request(permission_id), CancellationToken::new(|| false))
                .await
        }
    });

    tokio::task::yield_now().await;
    assert!(state.has_pending_permission(permission_id));
    state
        .resolve_permission(permission_id, PermissionDecision::AllowSession)
        .unwrap();

    assert_eq!(task.await.unwrap(), PermissionDecision::AllowSession);
    assert!(!state.has_pending_permission(permission_id));
}

#[tokio::test]
async fn cancellation_resolves_pending_permissions_as_deny() {
    let state = LocalAgentState::default();
    let resolver = state.permission_resolver();
    let cancelled = Arc::new(AtomicBool::new(false));
    let permission_id = Uuid::new_v4();

    let task = tokio::spawn({
        let resolver = resolver.clone();
        let cancelled = Arc::clone(&cancelled);
        async move {
            resolver
                .decide(
                    request(permission_id),
                    CancellationToken::new(move || cancelled.load(Ordering::SeqCst)),
                )
                .await
        }
    });

    tokio::task::yield_now().await;
    cancelled.store(true, Ordering::SeqCst);
    assert_eq!(task.await.unwrap(), PermissionDecision::Deny);
    assert!(!state.has_pending_permission(permission_id));
}

#[tokio::test]
async fn session_and_always_decisions_are_reused_for_matching_tool_arguments() {
    let state = LocalAgentState::default();
    let resolver = state.permission_resolver();
    let first_id = Uuid::new_v4();
    let first = resolver.decide(request(first_id), CancellationToken::new(|| false));
    tokio::pin!(first);
    tokio::task::yield_now().await;
    state
        .resolve_permission(first_id, PermissionDecision::AllowSession)
        .unwrap();
    assert_eq!(first.await, PermissionDecision::AllowSession);

    let second_id = Uuid::new_v4();
    let second = resolver.decide(request(second_id), CancellationToken::new(|| false));
    assert_eq!(
        tokio::time::timeout(Duration::from_millis(50), second)
            .await
            .unwrap(),
        PermissionDecision::AllowSession
    );

    let other_state = LocalAgentState::default();
    let other_resolver = other_state.permission_resolver();
    let third_id = Uuid::new_v4();
    let third = other_resolver.decide(request(third_id), CancellationToken::new(|| false));
    tokio::pin!(third);
    tokio::task::yield_now().await;
    other_state
        .resolve_permission(third_id, PermissionDecision::AllowAlways)
        .unwrap();
    assert_eq!(third.await, PermissionDecision::AllowAlways);

    let fourth_id = Uuid::new_v4();
    let fourth = other_resolver.decide(request(fourth_id), CancellationToken::new(|| false));
    assert_eq!(
        tokio::time::timeout(Duration::from_millis(50), fourth)
            .await
            .unwrap(),
        PermissionDecision::AllowAlways
    );
}

#[tokio::test]
async fn revoked_always_permission_is_not_reused_by_a_live_resolver() {
    let state = LocalAgentState::default();
    let resolver = state.permission_resolver();
    let first = request(Uuid::new_v4());
    let key = permission_key_for(first.tool_id.as_str(), &first.arguments);
    state.load_always_permission_keys([key.clone()]);

    assert_eq!(
        resolver
            .decide(first, CancellationToken::new(|| false))
            .await,
        PermissionDecision::AllowAlways
    );

    state.revoke_always_permission_key(&key);

    let second_id = Uuid::new_v4();
    let pending = resolver.decide(request(second_id), CancellationToken::new(|| false));
    tokio::pin!(pending);
    tokio::task::yield_now().await;
    assert!(state.has_pending_permission(second_id));
    state
        .resolve_permission(second_id, PermissionDecision::Deny)
        .unwrap();
    assert_eq!(pending.await, PermissionDecision::Deny);
}
