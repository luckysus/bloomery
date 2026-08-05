use bloomery::agent::protocol::export;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let targets = [
        (
            root.join("docs/protocol.schema.json"),
            export::json_schema(),
        ),
        (
            root.join("frontend/src/bridge/generated/protocol.ts"),
            export::typescript(),
        ),
    ];
    let check = env::args().any(|argument| argument == "--check");

    for (path, expected) in targets {
        if check {
            assert_fresh(&path, &expected);
        } else {
            write_generated(&path, &expected);
        }
    }
}

fn assert_fresh(path: &Path, expected: &str) {
    let actual = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read generated file {}: {error}", path.display()));
    assert_eq!(
        actual,
        expected,
        "generated file {} is stale; rerun the exporter",
        path.display()
    );
}

fn write_generated(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("create {}: {error}", parent.display()));
    }
    fs::write(path, contents)
        .unwrap_or_else(|error| panic!("write generated file {}: {error}", path.display()));
    println!("wrote {}", path.display());
}
