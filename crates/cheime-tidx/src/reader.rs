#![allow(unsafe_code)]

use crate::format::{self, read_block_entry, read_code_idx_entry, read_text};
use memmap2::Mmap;
use std::path::Path;

/// A read-only, mmap-backed view of a `.tidx` binary index file.
///
/// Provides O(log N) exact-code lookup and O(log N + K) prefix scanning
/// (where K = number of matching codes × entries per code).
pub struct TidexReader {
    _mmap: Mmap,
    // SAFETY: data lifetime backed by _mmap — TidexReader owns both.
    data: &'static [u8],
    code_idx_base: usize,
    code_count: u32,
    block_tbl_base: usize,
}

impl TidexReader {
    /// Open and mmap a `.tidx` file.
    pub fn open(path: &Path) -> Result<Self, TidexError> {
        let file = std::fs::File::open(path).map_err(|e| TidexError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

        let len = file
            .metadata()
            .map_err(|e| TidexError::Io {
                path: path.to_path_buf(),
                source: e,
            })?
            .len();

        if len == 0 {
            return Err(TidexError::EmptyFile {
                path: path.to_path_buf(),
            });
        }

        let mmap = unsafe {
            Mmap::map(&file).map_err(|e| TidexError::Io {
                path: path.to_path_buf(),
                source: e,
            })?
        };

        // SAFETY: The Mmap is stored in `_mmap` (stable address), so data
        // lifetime is valid for the struct's lifetime.
        let data: &'static [u8] = unsafe { std::slice::from_raw_parts(mmap.as_ptr(), mmap.len()) };

        let hdr = format::parse_header(data).map_err(|msg| TidexError::Format {
            path: path.to_path_buf(),
            reason: msg.to_string(),
        })?;

        // Validate declared sections fit within the file
        let file_len = data.len() as u64;
        let code_end = hdr.code_idx_off as u64 + hdr.code_count as u64 * 12;
        let block_end = hdr.block_tbl_off as u64 + hdr.entry_count as u64 * 8;
        let text_end = hdr.text_pool_off as u64 + hdr.text_pool_size as u64;
        if code_end > file_len || block_end > file_len || text_end > file_len {
            return Err(TidexError::Format {
                path: path.to_path_buf(),
                reason: format!(
                    "declared sections exceed file: code_end={}, block_end={}, text_end={}, file={}",
                    code_end, block_end, text_end, file_len
                ),
            });
        }

        Ok(Self {
            _mmap: mmap,
            data,
            code_idx_base: hdr.code_idx_off as usize,
            code_count: hdr.code_count,
            block_tbl_base: hdr.block_tbl_off as usize,
        })
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Read the code string for Level-1 entry `idx`.
    fn code_at(&self, idx: u32) -> &str {
        let (code_off, _, _) = read_code_idx_entry(self.data, self.code_idx_base, idx);
        // SAFETY: code_off was written by write_tidex and points to valid pool entry
        unsafe { read_text(self.data, code_off) }
    }

    /// Read the entry text and weight at Level-2 block index `blk_idx`.
    fn read_block(&self, blk_idx: u32) -> (&str, i32) {
        let (text_off, weight) = read_block_entry(self.data, self.block_tbl_base, blk_idx);
        // SAFETY: text_off was validated at write time
        let text = unsafe { read_text(self.data, text_off) };
        (text, weight)
    }

    // -----------------------------------------------------------------------
    // Binary search over Level-1 (sorted code strings)
    // -----------------------------------------------------------------------

    /// Binary search for `code`. Returns `(first_blk, count)` if exact match.
    pub fn find_code(&self, code: &str) -> Option<(u32, u32)> {
        if self.code_count == 0 {
            return None;
        }
        let mut lo = 0u32;
        let mut hi = self.code_count;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match self.code_at(mid).cmp(code) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => {
                    let (_, first_blk, count) =
                        read_code_idx_entry(self.data, self.code_idx_base, mid);
                    return Some((first_blk, count));
                }
            }
        }
        None
    }

    /// Find the starting index (in Level-1) of codes that begin with `prefix`.
    /// Returns `(start_idx, past_end_idx)` — i.e., the half-open range.
    pub fn find_prefix_range(&self, prefix: &str) -> (u32, u32) {
        if self.code_count == 0 {
            return (0, 0);
        }

        // Binary search for the first code >= prefix
        let start_idx = self.lower_bound(prefix);

        if start_idx >= self.code_count {
            return (0, 0);
        }

        // Walk forward through codes that start with prefix
        let mut end_idx = start_idx;
        while end_idx < self.code_count && self.code_at(end_idx).starts_with(prefix) {
            end_idx += 1;
        }

        (start_idx, end_idx)
    }

    /// Returns the index of the first code >= `target`.
    fn lower_bound(&self, target: &str) -> u32 {
        let mut lo = 0u32;
        let mut hi = self.code_count;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.code_at(mid) < target {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }

    // -----------------------------------------------------------------------
    // Query interface
    // -----------------------------------------------------------------------

    /// Exact query: all entries for `code`, sorted by weight desc.
    pub fn query(&self, code: &str) -> Vec<(String, i32)> {
        match self.find_code(code) {
            Some((first_blk, count)) => {
                let mut results = Vec::with_capacity(count as usize);
                for i in 0..count {
                    let (text, weight) = self.read_block(first_blk + i);
                    results.push((text.to_string(), weight));
                }
                results
            }
            None => Vec::new(),
        }
    }

    /// Prefix query: top-`limit` entries across all codes that start with `prefix`,
    /// sorted by weight descending.
    pub fn query_prefix(&self, prefix: &str, limit: usize) -> Vec<(String, i32)> {
        let (start_idx, end_idx) = self.find_prefix_range(prefix);
        if start_idx >= end_idx {
            return Vec::new();
        }

        // Collect all matching entries into a heap → top-N
        let mut all: Vec<(i32, String)> = Vec::new();
        for ci in start_idx..end_idx {
            let (first_blk, count) = {
                let (_, fb, cnt) = read_code_idx_entry(self.data, self.code_idx_base, ci);
                (fb, cnt)
            };
            for i in 0..count {
                let (text, weight) = self.read_block(first_blk + i);
                all.push((weight, text.to_string()));
            }
        }

        // Sort by weight desc, keep top limit
        all.sort_by_key(|(w, _)| std::cmp::Reverse(*w));
        all.truncate(limit);

        all.into_iter().map(|(w, t)| (t, w)).collect()
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum TidexError {
    #[error("I/O error opening {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("empty file: {path}")]
    EmptyFile { path: std::path::PathBuf },
    #[error("invalid format in {path}: {reason}")]
    Format {
        path: std::path::PathBuf,
        reason: String,
    },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::write_tidex;
    use tempfile::NamedTempFile;

    fn build_test_tidex(path: &Path) {
        let entries: &[(&str, &[(String, i32)])] = &[
            ("na", &[("那".to_string(), 100), ("拿".to_string(), 60)][..]),
            (
                "ni",
                &[
                    ("你".to_string(), 200),
                    ("呢".to_string(), 90),
                    ("尼".to_string(), 70),
                ][..],
            ),
            ("ni hao", &[("你好".to_string(), 300)][..]),
            ("ni men", &[("你们".to_string(), 150)][..]),
            ("za", &[("咋".to_string(), 50)][..]),
        ];
        write_tidex(path, entries).unwrap();
    }

    #[test]
    fn exact_query_finds_code() {
        let tmp = NamedTempFile::new().unwrap();
        build_test_tidex(tmp.path());
        let r = TidexReader::open(tmp.path()).unwrap();

        let results = r.query("ni");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, "你"); // weight 200
        assert_eq!(results[1].0, "呢"); // weight 90
        assert_eq!(results[2].0, "尼"); // weight 70
    }

    #[test]
    fn exact_query_missing_returns_empty() {
        let tmp = NamedTempFile::new().unwrap();
        build_test_tidex(tmp.path());
        let r = TidexReader::open(tmp.path()).unwrap();

        assert!(r.query("nonexistent").is_empty());
    }

    #[test]
    fn prefix_query_ni_matches_ni_ni_hao_ni_men() {
        let tmp = NamedTempFile::new().unwrap();
        build_test_tidex(tmp.path());
        let r = TidexReader::open(tmp.path()).unwrap();

        let results = r.query_prefix("ni", 10);
        // Entries from: ni (3), ni hao (1), ni men (1) = 5 total
        assert_eq!(results.len(), 5);
        // Should be sorted by weight desc: 你好(300), 你(200), 你们(150), 呢(90), 尼(70)
        assert_eq!(results[0].0, "你好");
        assert_eq!(results[1].0, "你");
        assert_eq!(results[2].0, "你们");
    }

    #[test]
    fn prefix_query_n_matches_na_ni_ni_hao_ni_men() {
        let tmp = NamedTempFile::new().unwrap();
        build_test_tidex(tmp.path());
        let r = TidexReader::open(tmp.path()).unwrap();

        let results = r.query_prefix("n", 20);
        // Entries: na(2) + ni(3) + ni hao(1) + ni men(1) = 7
        assert_eq!(results.len(), 7);
    }

    #[test]
    fn prefix_query_excludes_non_matching() {
        let tmp = NamedTempFile::new().unwrap();
        build_test_tidex(tmp.path());
        let r = TidexReader::open(tmp.path()).unwrap();

        let results = r.query_prefix("z", 10);
        assert_eq!(results.len(), 1); // only "za"
        assert_eq!(results[0].0, "咋");
    }

    #[test]
    fn prefix_query_empty_string_matches_all() {
        let tmp = NamedTempFile::new().unwrap();
        build_test_tidex(tmp.path());
        let r = TidexReader::open(tmp.path()).unwrap();

        let results = r.query_prefix("", 20);
        assert_eq!(results.len(), 8); // all entries: na(2) + ni(3) + ni hao(1) + ni men(1) + za(1) = 8
    }

    #[test]
    fn prefix_query_limited() {
        let tmp = NamedTempFile::new().unwrap();
        build_test_tidex(tmp.path());
        let r = TidexReader::open(tmp.path()).unwrap();

        let results = r.query_prefix("ni", 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "你好");
        assert_eq!(results[1].0, "你");
    }

    #[test]
    fn empty_file_error() {
        let tmp = NamedTempFile::new().unwrap();
        // write nothing — empty file
        let result = TidexReader::open(tmp.path());
        assert!(result.is_err());
    }
}
