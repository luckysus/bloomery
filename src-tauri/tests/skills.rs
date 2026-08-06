use bloomery::skills::{
    discover_skills, render_enabled_skills, summarize_skills, SkillErrorCode, SkillRecord,
    SkillRoot, SkillScope, SkillSource,
};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("bloomery-skills-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create temporary skill root");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_skill(root: &Path, name: &str, frontmatter: &str, body: &str) {
    let directory = root.join(name);
    fs::create_dir_all(&directory).expect("create skill directory");
    fs::write(
        directory.join("SKILL.md"),
        format!("---\n{frontmatter}\n---\n\n{body}\n"),
    )
    .expect("write skill");
}

fn root(scope: SkillScope, path: &Path) -> SkillRoot {
    SkillRoot::new(scope, path.to_path_buf())
}

#[test]
fn discovers_frontmatter_and_preserves_markdown_body() {
    let root_dir = TempRoot::new();
    write_skill(
        root_dir.path(),
        "steel-review",
        "name: steel-review\ndescription: Review steel process data\nversion: 1.2.0\ncompatibility: bloomery>=0.1.0",
        "# Review\n\nUse the local evidence before making a claim.",
    );

    let report = discover_skills(&[root(SkillScope::Workspace, root_dir.path())], "0.1.0");

    assert!(report.errors.is_empty());
    assert_eq!(report.skills.len(), 1);
    assert_eq!(report.skills[0].name, "steel-review");
    assert_eq!(report.skills[0].description, "Review steel process data");
    assert_eq!(report.skills[0].version, "1.2.0");
    assert_eq!(
        report.skills[0].body,
        "# Review\n\nUse the local evidence before making a claim."
    );
    assert_eq!(report.skills[0].source.scope, SkillScope::Workspace);
}

#[test]
fn accepts_claude_skill_frontmatter_without_optional_version() {
    let root_dir = TempRoot::new();
    write_skill(
        root_dir.path(),
        "claude-compatible",
        "name: claude-compatible\ndescription: Compatible with Claude Skills",
        "Follow the local evidence policy.",
    );

    let report = discover_skills(&[root(SkillScope::User, root_dir.path())], "0.1.0");

    assert!(report.errors.is_empty());
    assert_eq!(report.skills[0].version, "0.0.0");
}

#[test]
fn isolates_malformed_and_non_utf8_skills() {
    let root_dir = TempRoot::new();
    write_skill(
        root_dir.path(),
        "valid",
        "name: valid\ndescription: Valid\nversion: 1.0.0",
        "body",
    );
    fs::create_dir_all(root_dir.path().join("missing-frontmatter"))
        .expect("create malformed skill");
    fs::write(
        root_dir.path().join("missing-frontmatter/SKILL.md"),
        "name: broken\nbody",
    )
    .expect("write malformed skill");
    let invalid_directory = root_dir.path().join("invalid-utf8");
    fs::create_dir_all(&invalid_directory).expect("create invalid skill");
    let mut file =
        fs::File::create(invalid_directory.join("SKILL.md")).expect("create invalid file");
    file.write_all(&[0xff, 0xfe, 0xfd])
        .expect("write invalid UTF-8");

    let report = discover_skills(&[root(SkillScope::User, root_dir.path())], "0.1.0");

    assert_eq!(
        report
            .skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect::<Vec<_>>(),
        ["valid"]
    );
    assert!(report
        .errors
        .iter()
        .any(|error| error.code == SkillErrorCode::MissingFrontmatter));
    assert!(report
        .errors
        .iter()
        .any(|error| error.code == SkillErrorCode::InvalidUtf8));
}

#[test]
fn higher_precedence_scope_wins_and_incompatible_skill_is_rejected() {
    let user = TempRoot::new();
    let workspace = TempRoot::new();
    let domain = TempRoot::new();
    write_skill(
        user.path(),
        "steel",
        "name: steel\ndescription: User\nversion: 3.0.0",
        "user",
    );
    write_skill(
        workspace.path(),
        "steel",
        "name: steel\ndescription: Workspace\nversion: 2.0.0",
        "workspace",
    );
    write_skill(
        domain.path(),
        "steel",
        "name: steel\ndescription: Domain\nversion: 1.0.0",
        "domain",
    );
    write_skill(
        domain.path(),
        "future",
        "name: future\ndescription: Future\nversion: 1.0.0\ncompatibility: bloomery>=9.0.0",
        "future",
    );

    let report = discover_skills(
        &[
            root(SkillScope::Domain, domain.path()),
            root(SkillScope::User, user.path()),
            root(SkillScope::Workspace, workspace.path()),
        ],
        "0.1.0",
    );

    let steel = report
        .skills
        .iter()
        .find(|skill| skill.name == "steel")
        .expect("selected steel skill");
    assert_eq!(steel.description, "User");
    assert_eq!(steel.source.scope, SkillScope::User);
    assert!(!report.skills.iter().any(|skill| skill.name == "future"));
    assert!(report
        .errors
        .iter()
        .any(|error| error.code == SkillErrorCode::Incompatible));
}

#[test]
fn merged_skills_are_sorted_deterministically() {
    let root_dir = TempRoot::new();
    write_skill(
        root_dir.path(),
        "zeta",
        "name: zeta\ndescription: Z\nversion: 1.0.0",
        "z",
    );
    write_skill(
        root_dir.path(),
        "alpha",
        "name: alpha\ndescription: A\nversion: 1.0.0",
        "a",
    );

    let report = discover_skills(&[root(SkillScope::Domain, root_dir.path())], "0.1.0");

    assert_eq!(
        report
            .skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect::<Vec<_>>(),
        ["alpha", "zeta"]
    );
}

#[test]
fn enabled_skills_render_bounded_context_and_exact_versions() {
    let skill = SkillRecord {
        name: "steel-review".to_string(),
        description: "Review steel evidence".to_string(),
        version: "1.2.0".to_string(),
        compatibility: vec!["bloomery>=0.1.0".to_string()],
        body: "Use the source document.\n".repeat(4_000),
        source: SkillSource {
            scope: SkillScope::Workspace,
            path: PathBuf::from("workspace/.claude/skills/steel-review/SKILL.md"),
        },
        content_sha256: "abc123".to_string(),
    };

    let rendered = render_enabled_skills(&[skill.clone()], &BTreeSet::from([skill.name.clone()]));

    assert_eq!(rendered.enabled_versions, vec!["steel-review@1.2.0#abc123"]);
    assert!(rendered.prompt.contains("enabled_skills:"));
    assert!(rendered.prompt.contains("steel-review (v1.2.0)"));
    assert!(rendered.prompt.contains("Use the source document."));
    assert!(rendered.prompt.chars().count() <= 12_000);
}

#[test]
fn skill_summaries_expose_state_without_skill_body() {
    let skill = SkillRecord {
        name: "steel-review".to_string(),
        description: "Review steel evidence".to_string(),
        version: "1.0.0".to_string(),
        compatibility: Vec::new(),
        body: "private prompt body".to_string(),
        source: SkillSource {
            scope: SkillScope::User,
            path: PathBuf::from("user/.claude/skills/steel-review/SKILL.md"),
        },
        content_sha256: "def456".to_string(),
    };

    let summaries = summarize_skills(&[skill], &BTreeSet::from(["steel-review".to_string()]));
    let value = serde_json::to_value(&summaries).expect("serialize skill summaries");

    assert_eq!(value[0]["enabled"], true);
    assert_eq!(value[0]["content_sha256"], "def456");
    assert!(value[0].get("body").is_none());
}
