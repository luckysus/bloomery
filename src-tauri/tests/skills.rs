use bloomery::skills::{
    discover_skills, render_enabled_skills, render_relevant_skills, summarize_skills,
    SkillErrorCode, SkillRecord, SkillRoot, SkillScope, SkillSource,
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
        "name: steel-review\ndescription: Review steel process data\nversion: 1.2.0\ntags: [steel, review]\ncompatibility: bloomery>=0.1.0",
        "# Review\n\nUse the local evidence before making a claim.",
    );

    let report = discover_skills(&[root(SkillScope::User, root_dir.path())], "0.1.0");

    assert!(report.errors.is_empty());
    assert_eq!(report.skills.len(), 1);
    assert_eq!(report.skills[0].name, "steel-review");
    assert_eq!(report.skills[0].description, "Review steel process data");
    assert_eq!(report.skills[0].version, "1.2.0");
    assert_eq!(report.skills[0].tags, vec!["steel", "review"]);
    assert_eq!(
        report.skills[0].body,
        "# Review\n\nUse the local evidence before making a claim."
    );
    assert_eq!(report.skills[0].source.scope, SkillScope::User);
}

#[test]
fn accepts_skill_frontmatter_without_optional_version() {
    let root_dir = TempRoot::new();
    write_skill(
        root_dir.path(),
        "minimal-frontmatter",
        "name: minimal-frontmatter\ndescription: A reusable Bloomery Skill",
        "Follow the local evidence policy.",
    );

    let report = discover_skills(&[root(SkillScope::User, root_dir.path())], "0.1.0");

    assert!(report.errors.is_empty());
    assert_eq!(report.skills[0].version, "0.0.0");
}

#[test]
fn only_user_skill_scope_is_deserializable() {
    assert_eq!(
        serde_json::from_str::<SkillScope>(r#""user""#).expect("user scope"),
        SkillScope::User
    );
    assert!(serde_json::from_str::<SkillScope>(r#""workspace""#).is_err());
    assert!(serde_json::from_str::<SkillScope>(r#""domain""#).is_err());
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
fn duplicate_skill_is_isolated_and_incompatible_skill_is_rejected() {
    let first = TempRoot::new();
    let second = TempRoot::new();
    write_skill(
        first.path(),
        "steel",
        "name: steel\ndescription: First\nversion: 3.0.0",
        "first",
    );
    write_skill(
        second.path(),
        "steel",
        "name: steel\ndescription: Second\nversion: 2.0.0",
        "second",
    );
    write_skill(
        second.path(),
        "future",
        "name: future\ndescription: Future\nversion: 1.0.0\ncompatibility: bloomery>=9.0.0",
        "future",
    );

    let report = discover_skills(
        &[
            root(SkillScope::User, second.path()),
            root(SkillScope::User, first.path()),
        ],
        "0.1.0",
    );

    let steel = report
        .skills
        .iter()
        .find(|skill| skill.name == "steel")
        .expect("selected steel skill");
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

    let report = discover_skills(&[root(SkillScope::User, root_dir.path())], "0.1.0");

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
        tags: vec!["steel".to_string()],
        compatibility: vec!["bloomery>=0.1.0".to_string()],
        body: "Use the source document.\n".repeat(4_000),
        source: SkillSource {
            scope: SkillScope::User,
            path: PathBuf::from("user/.bloomery/skills/steel-review/SKILL.md"),
        },
        content_sha256: "abc123".to_string(),
    };

    let rendered = render_enabled_skills(&[skill.clone()], &BTreeSet::from([skill.name.clone()]));

    assert_eq!(rendered.enabled_versions, vec!["steel-review@1.2.0#abc123"]);
    assert_eq!(rendered.loaded.len(), 1);
    assert_eq!(rendered.loaded[0].name, "steel-review");
    assert_eq!(rendered.loaded[0].trigger_reason, "enabled_by_user");
    assert!(rendered.prompt.contains("enabled_skills:"));
    assert!(rendered.prompt.contains("steel-review (v1.2.0)"));
    assert!(rendered.prompt.contains("Use the source document."));
    assert!(rendered.prompt.chars().count() <= 12_000);
}

#[test]
fn query_render_loads_only_matching_enabled_skills() {
    let steel = SkillRecord {
        name: "steel-review".to_string(),
        description: "Review steel evidence".to_string(),
        version: "1.2.0".to_string(),
        tags: vec!["steel".to_string(), "evidence".to_string()],
        compatibility: Vec::new(),
        body: "Use steel evidence.".to_string(),
        source: SkillSource {
            scope: SkillScope::User,
            path: PathBuf::from("user/.bloomery/skills/steel-review/SKILL.md"),
        },
        content_sha256: "abc123".to_string(),
    };
    let writing = SkillRecord {
        name: "writing-polish".to_string(),
        description: "Polish release notes".to_string(),
        version: "2.0.0".to_string(),
        tags: vec!["writing".to_string()],
        compatibility: Vec::new(),
        body: "Improve prose.".to_string(),
        source: SkillSource {
            scope: SkillScope::User,
            path: PathBuf::from("user/.bloomery/skills/writing-polish/SKILL.md"),
        },
        content_sha256: "def456".to_string(),
    };
    let enabled = BTreeSet::from([steel.name.clone(), writing.name.clone()]);

    let rendered = render_relevant_skills(&[steel, writing], &enabled, "请审查 Q345 钢铁证据");

    assert_eq!(rendered.enabled_versions, vec!["steel-review@1.2.0#abc123"]);
    assert_eq!(rendered.loaded[0].trigger_reason, "matched_query_tag:steel");
    assert!(rendered.prompt.contains("Use steel evidence."));
    assert!(!rendered.prompt.contains("Improve prose."));
}

#[test]
fn skill_summaries_expose_state_without_skill_body() {
    let skill = SkillRecord {
        name: "steel-review".to_string(),
        description: "Review steel evidence".to_string(),
        version: "1.0.0".to_string(),
        tags: vec!["steel".to_string(), "evidence".to_string()],
        compatibility: Vec::new(),
        body: "private prompt body".to_string(),
        source: SkillSource {
            scope: SkillScope::User,
            path: PathBuf::from("user/.bloomery/skills/steel-review/SKILL.md"),
        },
        content_sha256: "def456".to_string(),
    };

    let summaries = summarize_skills(&[skill], &BTreeSet::from(["steel-review".to_string()]));
    let value = serde_json::to_value(&summaries).expect("serialize skill summaries");

    assert_eq!(value[0]["enabled"], true);
    assert_eq!(value[0]["tags"][0], "steel");
    assert_eq!(value[0]["content_sha256"], "def456");
    assert!(value[0].get("body").is_none());
}
