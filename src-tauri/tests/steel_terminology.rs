use bloomery::domains::load_package;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

fn steel_package_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../domain-packs/steel")
}

fn read_json(relative: &str) -> Value {
    let path = steel_package_root().join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| {
        panic!("parse {}: {error}", path.display())
    })
}

const ALLOWED_CATEGORIES: &[&str] = &[
    "steel_grade",
    "composition_element",
    "mechanical_property",
    "derived_index",
    "defect",
    "standard",
    "process_stage",
    "production_identifier",
    "semi_product",
];

const REQUIRED_CATEGORIES: &[&str] = &[
    "steel_grade",
    "composition_element",
    "mechanical_property",
    "defect",
    "standard",
    "process_stage",
];

const REQUIRED_PROCESS_STAGES: &[&str] = &["steelmaking", "refining", "casting", "heating", "rolling"];

fn terms<'a>(terminology: &'a Value) -> Vec<&'a Value> {
    terminology["terms"]
        .as_array()
        .expect("terminology must list terms")
        .iter()
        .collect()
}

fn text<'a>(term: &'a Value, field: &str) -> &'a str {
    term[field]
        .as_str()
        .unwrap_or_else(|| panic!("term is missing {field}"))
}

#[test]
fn terminology_licenses_and_schema_are_declared() {
    let terminology = read_json("assets/terminology.json");
    assert_eq!(terminology["license"], "Apache-2.0");
    assert_eq!(terminology["schema_version"], "1.1.0");
    assert!(!terminology["source_policy"].as_str().unwrap_or("").is_empty());
    assert!(!terms(&terminology).is_empty(), "terminology must not be empty");
}

#[test]
fn canonical_terms_and_ids_are_unique() {
    let terminology = read_json("assets/terminology.json");
    let mut ids = HashSet::new();
    let mut canonicals = HashSet::new();
    for term in terms(&terminology) {
        let id = text(term, "id");
        assert!(ids.insert(id.to_string()), "duplicate term id: {id}");
        let canonical = text(term, "canonical").to_lowercase();
        assert!(
            canonicals.insert(canonical.clone()),
            "duplicate canonical term: {canonical}"
        );
        assert!(
            !text(term, "definition").trim().is_empty(),
            "term {id} has no definition"
        );
    }
}

#[test]
fn aliases_are_unique_unless_disambiguated() {
    let terminology = read_json("assets/terminology.json");
    let disambiguated: HashSet<String> = terminology["disambiguation_rules"]
        .as_array()
        .expect("disambiguation_rules must be an array")
        .iter()
        .map(|rule| {
            let alias = rule["alias"]
                .as_str()
                .expect("disambiguation rule must name an alias");
            assert!(
                !rule["resolution"].as_str().unwrap_or("").trim().is_empty(),
                "disambiguation rule for {alias} has no resolution"
            );
            alias.to_lowercase()
        })
        .collect();

    let mut owners: HashMap<String, Vec<String>> = HashMap::new();
    for term in terms(&terminology) {
        let id = text(term, "id").to_string();
        let canonical = text(term, "canonical").to_lowercase();
        let aliases = term["aliases"]
            .as_array()
            .unwrap_or_else(|| panic!("term {id} must list aliases"));
        assert!(!aliases.is_empty(), "term {id} has no aliases");
        let mut local = HashSet::new();
        for alias in aliases {
            let alias = alias
                .as_str()
                .unwrap_or_else(|| panic!("term {id} has a non-string alias"))
                .to_lowercase();
            assert!(local.insert(alias.clone()), "term {id} repeats alias {alias}");
            assert!(
                alias != canonical,
                "term {id} aliases its own canonical form"
            );
            owners.entry(alias).or_default().push(id.clone());
        }
        owners.entry(canonical.clone()).or_default();
        let canonical_owners = owners.get_mut(&canonical).unwrap();
        if !canonical_owners.contains(&id) {
            canonical_owners.push(id.clone());
        }
    }

    for (alias, owners) in &owners {
        if owners.len() > 1 {
            assert!(
                disambiguated.contains(alias),
                "ambiguous alias {alias} shared by {:?} has no disambiguation rule",
                owners
            );
        }
    }
    for alias in &disambiguated {
        let shared = owners.get(alias).map(Vec::len).unwrap_or(0) > 1;
        assert!(shared, "disambiguation rule for {alias} is unused");
    }
}

#[test]
fn categories_units_and_stages_cover_the_steel_domain() {
    let terminology = read_json("assets/terminology.json");
    let mut categories = HashSet::new();
    let mut stages = HashSet::new();
    for term in terms(&terminology) {
        let id = text(term, "id");
        let category = text(term, "category");
        assert!(
            ALLOWED_CATEGORIES.contains(&category),
            "term {id} uses unknown category {category}"
        );
        categories.insert(category.to_string());
        if matches!(category, "composition_element" | "mechanical_property") {
            let units = term["units"]
                .as_array()
                .unwrap_or_else(|| panic!("term {id} must declare units"));
            assert!(
                !units.is_empty(),
                "term {id} must declare at least one unit"
            );
            for unit in units {
                assert!(
                    !unit.as_str().unwrap_or("").trim().is_empty(),
                    "term {id} has an empty unit"
                );
            }
        }
        if category == "process_stage" {
            stages.insert(text(term, "stage").to_string());
        }
    }

    for required in REQUIRED_CATEGORIES {
        assert!(
            categories.contains(*required),
            "terminology is missing category {required}"
        );
    }
    for stage in REQUIRED_PROCESS_STAGES {
        assert!(
            stages.contains(*stage),
            "terminology is missing process stage {stage}"
        );
    }
}

#[test]
fn every_term_source_resolves_to_a_ledger_entry_without_restricted_text() {
    let terminology = read_json("assets/terminology.json");
    let ledger = read_json("assets/source-ledger.json");
    assert_eq!(ledger["license"], "Apache-2.0");
    assert!(!ledger["policy"].as_str().unwrap_or("").is_empty());

    let mut entries = HashSet::new();
    for entry in ledger["entries"].as_array().expect("ledger must list entries") {
        let id = entry["id"].as_str().expect("ledger entry needs id");
        assert!(entries.insert(id.to_string()), "duplicate ledger entry {id}");
        assert!(
            !entry["title"].as_str().unwrap_or("").trim().is_empty(),
            "ledger entry {id} has no title"
        );
        assert!(
            !entry["publisher"].as_str().unwrap_or("").trim().is_empty(),
            "ledger entry {id} has no publisher"
        );
        assert!(
            !entry["license"].as_str().unwrap_or("").trim().is_empty(),
            "ledger entry {id} has no license"
        );
        assert_eq!(
            entry["restricted_text_redistributed"],
            Value::Bool(false),
            "ledger entry {id} must not redistribute restricted text"
        );
    }

    for term in terms(&terminology) {
        let id = text(term, "id");
        let source = text(term, "source");
        assert!(
            entries.contains(source),
            "term {id} references unknown ledger source {source}"
        );
    }
}

#[test]
fn steel_package_loads_with_pinned_terminology_and_ledger_assets() {
    let package = load_package(&steel_package_root(), env!("CARGO_PKG_VERSION"))
        .expect("official steel package must load with pinned hashes");

    let kinds: HashSet<String> = package
        .assets
        .iter()
        .map(|asset| asset.spec.kind.clone())
        .collect();
    assert!(kinds.contains("terminology"));
    assert!(kinds.contains("source_ledger"));
    for asset in &package.assets {
        assert!(
            asset.spec.sha256.is_some(),
            "official package asset {} must pin a SHA-256",
            asset.spec.path
        );
    }
}
