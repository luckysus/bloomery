use super::{RagTaskError, StoredObjectRef};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Clone)]
pub(super) struct ContentStore {
    root: PathBuf,
}

impl ContentStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn read(&self, object: &StoredObjectRef, limit: u64) -> Result<Vec<u8>, RagTaskError> {
        let path = self.path(object);
        let metadata = fs::metadata(&path).map_err(|error| io("read object metadata", error))?;
        if !metadata.is_file() || metadata.len() > limit {
            return Err(RagTaskError::new(
                "content_store_invalid",
                "stored object is missing, not a file, or exceeds its size limit",
            ));
        }
        let mut file = File::open(&path).map_err(|error| io("open object", error))?;
        let capacity = usize::try_from(metadata.len()).map_err(|_| {
            RagTaskError::new("content_store_invalid", "stored object is too large")
        })?;
        let mut bytes = Vec::with_capacity(capacity);
        file.read_to_end(&mut bytes)
            .map_err(|error| io("read object", error))?;
        verify(&bytes, object.sha256())?;
        Ok(bytes)
    }

    pub fn put(&self, bytes: &[u8]) -> Result<StoredObjectRef, RagTaskError> {
        let hash = format!("{:x}", Sha256::digest(bytes));
        let object =
            StoredObjectRef::new(&hash, format!("objects/sha256/{}/{}", &hash[..2], hash))?;
        let target = self.path(&object);
        if target.exists() {
            verify_file(&target, bytes.len() as u64, object.sha256())?;
            return Ok(object);
        }
        let parent = target.parent().expect("content object has a parent");
        fs::create_dir_all(parent).map_err(|error| io("create object directory", error))?;
        let staging = parent.join(format!(".{}.tmp", Uuid::new_v4()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&staging)
                .map_err(|error| io("create staged object", error))?;
            file.write_all(bytes)
                .map_err(|error| io("write staged object", error))?;
            file.sync_all()
                .map_err(|error| io("sync staged object", error))?;
            drop(file);
            match fs::rename(&staging, &target) {
                Ok(()) => Ok(()),
                Err(_) if target.exists() => {
                    verify_file(&target, bytes.len() as u64, object.sha256())
                }
                Err(error) => Err(io("persist staged object", error)),
            }
        })();
        let _ = fs::remove_file(&staging);
        result?;
        Ok(object)
    }

    fn path(&self, object: &StoredObjectRef) -> PathBuf {
        self.root.join(Path::new(object.storage_key()))
    }
}

fn verify_file(path: &Path, expected_len: u64, expected_hash: &str) -> Result<(), RagTaskError> {
    let metadata = fs::metadata(path).map_err(|error| io("read existing object", error))?;
    if !metadata.is_file() || metadata.len() != expected_len {
        return Err(corrupt());
    }
    let mut file = File::open(path).map_err(|error| io("open existing object", error))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| io("read existing object", error))?;
    verify(&bytes, expected_hash)
}

fn verify(bytes: &[u8], expected_hash: &str) -> Result<(), RagTaskError> {
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual == expected_hash {
        Ok(())
    } else {
        Err(corrupt())
    }
}

fn corrupt() -> RagTaskError {
    RagTaskError::new(
        "content_store_corrupt",
        "stored object does not match its content-addressed key",
    )
}

fn io(action: &str, error: std::io::Error) -> RagTaskError {
    RagTaskError::new("content_store_io", format!("{action}: {error}"))
}
