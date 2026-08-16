#![cfg(windows)]

use bloomery::permissions::path::{
    authorize_existing_file_with_handle, authorize_existing_path, authorize_output_path,
    AuthorizedRoots, PathAuthorizationError,
};
use std::{
    fs::File,
    path::{Path, PathBuf},
};

fn fixture() -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("bloomery-permission-{}", uuid::Uuid::new_v4()));
    let outside = std::env::temp_dir().join(format!("bloomery-outside-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("nested")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(root.join("nested").join("inside.txt"), "inside").unwrap();
    std::fs::write(outside.join("outside.txt"), "outside").unwrap();
    (root, outside)
}

#[test]
fn relative_paths_are_rejected() {
    let (root, outside) = fixture();
    let roots = AuthorizedRoots::new(vec![root.clone()]).unwrap();

    assert!(matches!(
        roots.authorize(Path::new("nested\\inside.txt")),
        Err(PathAuthorizationError::RelativePath)
    ));
    cleanup(root, outside);
}

#[test]
fn parent_escape_is_rejected_after_canonicalization() {
    let (root, outside) = fixture();
    let roots = AuthorizedRoots::new(vec![root.clone()]).unwrap();

    let escaped = root
        .join("nested")
        .join("..")
        .join("..")
        .join(
            outside
                .file_name()
                .expect("outside directory name")
                .to_os_string(),
        )
        .join("outside.txt");
    assert!(matches!(
        roots.authorize(&escaped),
        Err(PathAuthorizationError::OutsideRoots)
    ));
    cleanup(root, outside);
}

#[test]
fn device_namespaces_and_alternate_streams_are_rejected() {
    let (root, outside) = fixture();
    let roots = AuthorizedRoots::new(vec![root.clone()]).unwrap();

    assert!(matches!(
        roots.authorize(Path::new(r"\\.\C:\nested\inside.txt")),
        Err(PathAuthorizationError::DevicePath)
    ));
    assert!(matches!(
        roots.authorize(Path::new(r"\\?\C:\nested\inside.txt")),
        Err(PathAuthorizationError::DevicePath)
    ));
    assert!(matches!(
        roots.authorize(&root.join("nested").join("inside.txt:secret")),
        Err(PathAuthorizationError::AlternateDataStream)
    ));
    cleanup(root, outside);
}

#[test]
fn unc_paths_are_rejected() {
    let (root, outside) = fixture();
    let roots = AuthorizedRoots::new(vec![root.clone()]).unwrap();

    assert!(matches!(
        roots.authorize(Path::new(r"\\server\share\file.txt")),
        Err(PathAuthorizationError::NetworkPath)
    ));
    cleanup(root, outside);
}

#[test]
fn existing_and_nonexistent_targets_inside_a_root_are_authorized() {
    let (root, outside) = fixture();
    let roots = AuthorizedRoots::new(vec![root.clone()]).unwrap();

    let existing = roots
        .authorize(&root.join("nested").join("inside.txt"))
        .unwrap();
    let missing = roots
        .authorize(&root.join("nested").join("new-output.txt"))
        .unwrap();
    assert!(existing.canonical_path().ends_with("inside.txt"));
    assert!(missing.canonical_path().ends_with("new-output.txt"));
    cleanup(root, outside);
}

#[test]
fn case_differences_do_not_escape_a_windows_root() {
    let (root, outside) = fixture();
    let roots = AuthorizedRoots::new(vec![root.clone()]).unwrap();
    let path = root.join("NESTED").join("INSIDE.TXT");

    let authorized = roots.authorize(&path).unwrap();
    assert!(authorized.canonical_path().ends_with("inside.txt"));
    cleanup(root, outside);
}

#[test]
fn opened_target_is_rechecked_before_effects() {
    let (root, outside) = fixture();
    let roots = AuthorizedRoots::new(vec![root.clone()]).unwrap();
    let inside = root.join("nested").join("inside.txt");
    let authorized = roots.authorize(&inside).unwrap();

    roots.verify_target(&authorized, &inside).unwrap();
    assert!(matches!(
        roots.verify_target(&authorized, &outside.join("outside.txt")),
        Err(PathAuthorizationError::TargetChanged)
    ));
    cleanup(root, outside);
}

#[test]
fn symlink_target_outside_root_is_rejected() {
    let (root, outside) = fixture();
    let link = root.join("nested").join("linked.txt");
    if std::os::windows::fs::symlink_file(outside.join("outside.txt"), &link).is_err() {
        cleanup(root, outside);
        return;
    }
    let roots = AuthorizedRoots::new(vec![root.clone()]).unwrap();

    assert!(matches!(
        roots.authorize(&link),
        Err(PathAuthorizationError::OutsideRoots)
    ));
    cleanup(root, outside);
}

#[test]
fn reparse_directory_target_outside_root_is_rejected() {
    let (root, outside) = fixture();
    let link = root.join("nested").join("linked-directory");
    if std::os::windows::fs::symlink_dir(&outside, &link).is_err() {
        cleanup(root, outside);
        return;
    }
    let roots = AuthorizedRoots::new(vec![root.clone()]).unwrap();

    assert!(matches!(
        roots.authorize(&link.join("outside.txt")),
        Err(PathAuthorizationError::OutsideRoots)
    ));
    cleanup(root, outside);
}

#[test]
fn opened_file_handle_is_rechecked_before_effects() {
    let (root, outside) = fixture();
    let roots = AuthorizedRoots::new(vec![root.clone()]).unwrap();
    let inside = root.join("nested").join("inside.txt");
    let authorized = roots.authorize(&inside).unwrap();
    let inside_file = File::open(&inside).unwrap();
    roots.verify_opened_file(&authorized, &inside_file).unwrap();

    let outside_file = File::open(outside.join("outside.txt")).unwrap();
    assert!(matches!(
        roots.verify_opened_file(&authorized, &outside_file),
        Err(PathAuthorizationError::TargetChanged)
    ));
    cleanup(root, outside);
}

#[test]
fn path_helpers_authorize_existing_and_new_sibling_targets() {
    let (root, outside) = fixture();
    let existing = root.join("nested").join("inside.txt");
    let output = root.join("nested").join("new-output.txt");

    let authorized_existing = authorize_existing_path(&existing).unwrap();
    let authorized_output = authorize_output_path(&output).unwrap();

    assert!(authorized_existing.canonical_path().ends_with("inside.txt"));
    assert!(authorized_output
        .canonical_path()
        .ends_with("new-output.txt"));
    cleanup(root, outside);
}

#[test]
fn authorized_file_helper_returns_a_handle_that_was_rechecked() {
    let (root, outside) = fixture();
    let inside = root.join("nested").join("inside.txt");

    let (authorized, file) = authorize_existing_file_with_handle(&inside).unwrap();

    assert!(authorized.canonical_path().ends_with("inside.txt"));
    assert!(file.metadata().unwrap().is_file());
    cleanup(root, outside);
}

fn cleanup(root: PathBuf, outside: PathBuf) {
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}
