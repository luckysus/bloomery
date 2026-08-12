use super::{ParseError, ParseLimits};
use std::collections::HashSet;
use std::io::{Cursor, Read};
use zip::ZipArchive;

pub(crate) struct OfficeArchive<'a> {
    archive: ZipArchive<Cursor<&'a [u8]>>,
}

impl<'a> OfficeArchive<'a> {
    pub fn open(bytes: &'a [u8], limits: ParseLimits) -> Result<Self, ParseError> {
        let mut archive = ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| ParseError::new("invalid_archive", error.to_string()))?;
        let raw_entry_count = central_directory_entry_count(
            bytes,
            usize::try_from(archive.central_directory_start()).map_err(|_| {
                ParseError::new("invalid_archive", "central directory offset is too large")
            })?,
        )?;
        if raw_entry_count != archive.len() {
            return Err(ParseError::new(
                "archive_duplicate_entry",
                "Office archive contains duplicate entry names",
            ));
        }
        if archive.len() > limits.max_archive_entries {
            return Err(ParseError::new(
                "archive_too_many_entries",
                "Office archive contains too many entries",
            ));
        }
        let mut expanded = 0u64;
        let mut names = HashSet::with_capacity(archive.len());
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|error| ParseError::new("invalid_archive", error.to_string()))?;
            if entry.enclosed_name().is_none() || entry.name().contains('\\') {
                return Err(ParseError::new(
                    "archive_path_traversal",
                    "Office archive contains an unsafe entry path",
                ));
            }
            if entry.is_symlink() {
                return Err(ParseError::new(
                    "archive_symlink",
                    "Office archive contains a symbolic link",
                ));
            }
            if !names.insert(entry.name().to_string()) {
                return Err(ParseError::new(
                    "archive_duplicate_entry",
                    "Office archive contains duplicate entry names",
                ));
            }
            expanded = expanded.checked_add(entry.size()).ok_or_else(|| {
                ParseError::new("archive_expanded_too_large", "archive size overflow")
            })?;
            if expanded > limits.max_expanded_bytes {
                return Err(ParseError::new(
                    "archive_expanded_too_large",
                    "Office archive exceeds the expanded-byte limit",
                ));
            }
        }
        Ok(Self { archive })
    }

    pub fn read_required(&mut self, name: &str) -> Result<Vec<u8>, ParseError> {
        self.read_optional(name)?.ok_or_else(|| {
            ParseError::new(
                "missing_archive_entry",
                format!("Office archive is missing {name}"),
            )
        })
    }

    pub fn read_required_limited(
        &mut self,
        name: &str,
        max_bytes: usize,
    ) -> Result<Vec<u8>, ParseError> {
        self.read_optional_limited(name, max_bytes)?.ok_or_else(|| {
            ParseError::new(
                "missing_archive_entry",
                format!("Office archive is missing {name}"),
            )
        })
    }

    pub fn read_optional(&mut self, name: &str) -> Result<Option<Vec<u8>>, ParseError> {
        self.read_optional_limited(name, usize::MAX)
    }

    pub fn read_optional_limited(
        &mut self,
        name: &str,
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, ParseError> {
        let Ok(mut entry) = self.archive.by_name(name) else {
            return Ok(None);
        };
        let capacity = usize::try_from(entry.size())
            .map_err(|_| ParseError::new("archive_expanded_too_large", "entry is too large"))?;
        if capacity > max_bytes {
            return Err(ParseError::new(
                "archive_entry_too_large",
                format!("Office archive entry exceeds the {max_bytes}-byte limit"),
            ));
        }
        let mut bytes = Vec::with_capacity(capacity);
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| ParseError::new("invalid_archive", error.to_string()))?;
        if bytes.len() > max_bytes {
            return Err(ParseError::new(
                "archive_entry_too_large",
                format!("Office archive entry exceeds the {max_bytes}-byte limit"),
            ));
        }
        Ok(Some(bytes))
    }

    pub fn read_prefix(&mut self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, ParseError> {
        let mut names = Vec::new();
        for index in 0..self.archive.len() {
            let entry = self
                .archive
                .by_index(index)
                .map_err(|error| ParseError::new("invalid_archive", error.to_string()))?;
            if !entry.is_dir() && entry.name().starts_with(prefix) {
                names.push(entry.name().to_string());
            }
        }
        names.sort();
        names
            .into_iter()
            .map(|name| self.read_required(&name).map(|bytes| (name, bytes)))
            .collect()
    }

    pub fn names(&mut self) -> Vec<String> {
        let mut names = self
            .archive
            .file_names()
            .map(str::to_string)
            .collect::<Vec<_>>();
        names.sort();
        names
    }
}

fn central_directory_entry_count(bytes: &[u8], mut cursor: usize) -> Result<usize, ParseError> {
    let mut count = 0usize;
    while bytes.get(cursor..cursor + 4) == Some(b"PK\x01\x02") {
        let header = bytes.get(cursor..cursor + 46).ok_or_else(|| {
            ParseError::new("invalid_archive", "central directory header is truncated")
        })?;
        let name_len = u16::from_le_bytes([header[28], header[29]]) as usize;
        let extra_len = u16::from_le_bytes([header[30], header[31]]) as usize;
        let comment_len = u16::from_le_bytes([header[32], header[33]]) as usize;
        cursor = cursor
            .checked_add(46)
            .and_then(|value| value.checked_add(name_len))
            .and_then(|value| value.checked_add(extra_len))
            .and_then(|value| value.checked_add(comment_len))
            .filter(|value| *value <= bytes.len())
            .ok_or_else(|| {
                ParseError::new("invalid_archive", "central directory entry is truncated")
            })?;
        count += 1;
    }
    Ok(count)
}
