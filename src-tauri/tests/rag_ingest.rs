use bloomery::rag::ingest::{ingest_file, IngestLimits};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("bloomery-rag-ingest-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).expect("write source fixture");
        path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove test directory");
    }
}

fn limits(max_bytes: u64) -> IngestLimits {
    IngestLimits { max_bytes }
}

#[test]
fn rejects_mime_and_extension_disagreement() {
    let directory = TestDirectory::new();
    let store = directory.path().join("store");
    let pdf_as_text = directory.file("standard.txt", b"%PDF-1.7\nbody");
    let text_as_pdf = directory.file("standard.pdf", b"Q355B yield strength");

    assert_eq!(
        ingest_file(&pdf_as_text, &store, limits(1024))
            .unwrap_err()
            .code(),
        "mime_extension_mismatch"
    );
    assert_eq!(
        ingest_file(&text_as_pdf, &store, limits(1024))
            .unwrap_err()
            .code(),
        "mime_extension_mismatch"
    );
}

#[test]
fn duplicate_bytes_reuse_one_content_addressed_object() {
    let directory = TestDirectory::new();
    let store = directory.path().join("store");
    let first = directory.file("standard.pdf", b"%PDF-1.7\nsame bytes");
    let second = directory.file(
        "\u{6807}\u{51c6}\u{526f}\u{672c}.pdf",
        b"%PDF-1.7\nsame bytes",
    );

    let created = ingest_file(&first, &store, limits(1024)).expect("ingest first file");
    let duplicate = ingest_file(&second, &store, limits(1024)).expect("ingest duplicate file");

    assert!(!created.duplicate);
    assert!(duplicate.duplicate);
    assert_eq!(created.content_sha256, duplicate.content_sha256);
    assert_eq!(created.stored_path, duplicate.stored_path);
    assert_eq!(created.storage_key, duplicate.storage_key);
    assert_eq!(
        created
            .stored_path
            .file_name()
            .and_then(|name| name.to_str()),
        Some(created.content_sha256.as_str())
    );
    assert!(!created.storage_key.contains("standard"));
    assert!(!created.storage_key.contains("\u{6807}\u{51c6}"));
}

#[test]
fn rejects_zero_and_oversized_files_without_storing_partial_objects() {
    let directory = TestDirectory::new();
    let store = directory.path().join("store");
    let empty = directory.file("empty.txt", b"");
    let oversized = directory.file("large.txt", b"123456789");

    assert_eq!(
        ingest_file(&empty, &store, limits(8)).unwrap_err().code(),
        "empty_file"
    );
    assert_eq!(
        ingest_file(&oversized, &store, limits(8))
            .unwrap_err()
            .code(),
        "file_too_large"
    );
    assert!(!store.join("objects").exists());
}

#[test]
fn changed_file_content_creates_a_new_immutable_object() {
    let directory = TestDirectory::new();
    let store = directory.path().join("store");
    let source = directory.file("notes.txt", b"heat one");
    let first = ingest_file(&source, &store, limits(1024)).expect("ingest first content");
    fs::write(&source, b"heat two").expect("change source content");

    let changed = ingest_file(&source, &store, limits(1024)).expect("ingest changed content");

    assert_ne!(first.content_sha256, changed.content_sha256);
    assert_ne!(first.stored_path, changed.stored_path);
    assert!(!changed.duplicate);
    assert_eq!(fs::read(first.stored_path).unwrap(), b"heat one");
    assert_eq!(fs::read(changed.stored_path).unwrap(), b"heat two");
}

#[test]
fn rejects_unsupported_formats() {
    let directory = TestDirectory::new();
    let source = directory.file("program.exe", b"MZ\x90\0");

    assert_eq!(
        ingest_file(&source, &directory.path().join("store"), limits(1024))
            .unwrap_err()
            .code(),
        "unsupported_format"
    );
}

#[test]
fn accepts_chinese_filenames_without_using_them_as_storage_paths() {
    let directory = TestDirectory::new();
    let source = directory.file(
        "\u{9ad8}\u{7089}\u{70bc}\u{94c1}\u{8bb0}\u{5f55}.md",
        "# \u{9ad8}\u{7089}\n\n\u{7089}\u{6e29} 1,500 C".as_bytes(),
    );

    let stored = ingest_file(&source, &directory.path().join("store"), limits(1024))
        .expect("ingest Chinese filename");

    assert_eq!(stored.mime_type, "text/markdown");
    assert!(!stored.storage_key.contains("\u{9ad8}\u{7089}"));
    assert_eq!(
        fs::read(stored.stored_path).unwrap(),
        fs::read(source).unwrap()
    );
}

#[test]
fn accepts_utf8_text_when_the_sniff_window_splits_a_character() {
    let directory = TestDirectory::new();
    let mut bytes = vec![b'a'; 8 * 1024 - 1];
    bytes.extend_from_slice("钢铁".as_bytes());
    let source = directory.file("boundary.txt", &bytes);

    let stored = ingest_file(&source, &directory.path().join("store"), limits(16 * 1024))
        .expect("ingest valid UTF-8 across sniff boundary");

    assert_eq!(stored.byte_len, bytes.len() as u64);
    assert_eq!(stored.mime_type, "text/plain");
}
