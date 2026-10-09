use rusqlite::Connection;
use serde_json::json;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use suna::agent::runtime::{CancellationToken, SkillTool, ToolExecutor, ToolInvocation};
use suna::skills::{save_enabled_names, SkillRoot, SkillScope};
use suna::storage::migrations::migrate;
use uuid::Uuid;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("suna-skill-tool-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("skills/enabled-skill")).unwrap();
        fs::create_dir_all(root.join("skills/disabled-skill")).unwrap();
        for name in ["enabled-skill", "disabled-skill"] {
            fs::write(root.join(format!("skills/{name}/SKILL.md")), format!("---\nname: {name}\ndescription: Skill fixture\nversion: 1.0.0\n---\n\nSnapshot content." )).unwrap();
        }
        Self(root)
    }

    fn connection(&self) -> Connection {
        let mut connection = Connection::open(self.0.join("skills.sqlite3")).unwrap();
        migrate(&mut connection).unwrap();
        connection
    }

    fn tools(&self, connection: &Connection, workspace: &str) -> SkillTool {
        SkillTool::for_workspace(
            connection,
            workspace,
            &[SkillRoot::new(SkillScope::User, self.0.join("skills"))],
        )
        .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn invocation(name: &str) -> ToolInvocation {
    ToolInvocation {
        tool_call_id: Uuid::new_v4(),
        tool_id: "agent.load_skill".to_string(),
        tool_name: "load_skill".to_string(),
        arguments: json!({"name": name}),
    }
}

#[tokio::test]
async fn load_skill_cannot_load_disabled_or_other_workspace_skills() {
    let fixture = Fixture::new();
    let mut connection = fixture.connection();
    save_enabled_names(
        &mut connection,
        "workspace-a",
        &BTreeSet::from(["enabled-skill".to_string()]),
    )
    .unwrap();
    let tools = fixture.tools(&connection, "workspace-a");
    let result = tools
        .execute(
            invocation("enabled-skill"),
            CancellationToken::new(|| false),
        )
        .await
        .unwrap();
    assert_eq!(result["content"], "Snapshot content.");
    assert_eq!(result["version"], "1.0.0");
    assert_eq!(
        tools
            .execute(
                invocation("disabled-skill"),
                CancellationToken::new(|| false)
            )
            .await
            .unwrap_err()
            .code,
        "skill_not_enabled"
    );
    assert_eq!(
        fixture
            .tools(&connection, "workspace-b")
            .execute(
                invocation("enabled-skill"),
                CancellationToken::new(|| false)
            )
            .await
            .unwrap_err()
            .code,
        "skill_not_enabled"
    );
    assert_eq!(
        SkillTool::default()
            .execute(
                invocation("enabled-skill"),
                CancellationToken::new(|| false)
            )
            .await
            .unwrap_err()
            .code,
        "skill_not_enabled"
    );
}

#[tokio::test]
async fn revocation_is_immediate_and_new_content_requires_a_new_snapshot() {
    let fixture = Fixture::new();
    let mut connection = fixture.connection();
    save_enabled_names(
        &mut connection,
        "workspace-a",
        &BTreeSet::from(["enabled-skill".to_string()]),
    )
    .unwrap();
    let tools = fixture.tools(&connection, "workspace-a");
    fs::write(
        fixture.0.join("skills/enabled-skill/SKILL.md"),
        "---\nname: enabled-skill\ndescription: Updated\nversion: 2.0.0\n---\n\nUpdated content.",
    )
    .unwrap();
    let original = tools
        .execute(
            invocation("enabled-skill"),
            CancellationToken::new(|| false),
        )
        .await
        .unwrap();
    assert_eq!(original["version"], "1.0.0");
    save_enabled_names(&mut connection, "workspace-a", &BTreeSet::new()).unwrap();
    assert_eq!(
        tools
            .execute(
                invocation("enabled-skill"),
                CancellationToken::new(|| false)
            )
            .await
            .unwrap_err()
            .code,
        "skill_not_enabled"
    );
    save_enabled_names(
        &mut connection,
        "workspace-a",
        &BTreeSet::from(["enabled-skill".to_string(), "disabled-skill".to_string()]),
    )
    .unwrap();
    assert_eq!(
        tools
            .execute(
                invocation("disabled-skill"),
                CancellationToken::new(|| false)
            )
            .await
            .unwrap_err()
            .code,
        "skill_not_enabled"
    );
    let fresh = fixture.tools(&connection, "workspace-a");
    assert_eq!(
        fresh
            .execute(
                invocation("enabled-skill"),
                CancellationToken::new(|| false)
            )
            .await
            .unwrap()["version"],
        "2.0.0"
    );
    assert!(fresh
        .execute(
            invocation("disabled-skill"),
            CancellationToken::new(|| false)
        )
        .await
        .is_ok());
}

#[tokio::test]
async fn skill_tool_rejects_path_arguments_and_cancelled_loads() {
    let tools = SkillTool::new();
    assert_eq!(
        tools
            .execute(invocation("../outside"), CancellationToken::new(|| false))
            .await
            .unwrap_err()
            .code,
        "invalid_skill"
    );
    let mut request = invocation("enabled-skill");
    request.arguments = json!({"name": "enabled-skill", "path": "outside/SKILL.md"});
    assert_eq!(
        tools
            .execute(request, CancellationToken::new(|| false))
            .await
            .unwrap_err()
            .code,
        "invalid_skill"
    );
    assert!(
        tools
            .execute(invocation("enabled-skill"), CancellationToken::new(|| true))
            .await
            .unwrap_err()
            .cancelled
    );
}
