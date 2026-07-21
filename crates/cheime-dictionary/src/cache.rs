//! Dictionary build cache with hash verification and fragment stitching.
//!
//! CheIME advantage: like cargo's incremental compilation, dictionary
//! sources are individually hashed. Only files whose hash changed are
//! re-parsed and re-compiled. Unchanged fragments are loaded from cache
//! and merged at deploy time. No full rebuild ≈ no wasted work.
//!
//! ## Cache layout
//! ```text
//! cache/
//!   dicts/
//!     rime_ice_base/
//!       a1b2c3d4.bin   ← compiled fragment (rmp-serde)
//!       e5f6g7h8.bin
//!     luna_pinyin/
//!       ...
//!   manifest.json       ← track file→hash mapping
//! ```
//!
//! ## Flow
//! 1. Compute SHA256 of each source `.dict.yaml` file
//! 2. Look up `cache/dicts/<name>/<hash>.bin`
//! 3. If exists → load from cache (deserialize BTreeMap fragment)
//! 4. If missing → parse .dict.yaml, compile, serialize to cache
//! 5. Merge all fragments into final CompiledIndex

use crate::body::{parse_body, DictEntry};
use crate::index::CompiledIndex;
use crate::DictColumn;
use cheime_model::DeploymentGeneration;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

// ── CacheFragment — serializable compiled fragment ─────────────────

/// A compiled fragment that can be cached to disk and later stitched.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct CacheFragment {
    /// Number of entries in this fragment.
    total_entries: usize,
    /// Source hash of the input file (for verification).
    source_hash: String,
    /// Compiled entries: code → list of (text, weight).
    entries: BTreeMap<String, Vec<(String, Option<i64>)>>,
}


// ── DictCache ──────────────────────────────────────────────────────

pub struct DictCache {
    cache_dir: PathBuf,
}

impl DictCache {
    /// Create a new cache rooted at `cache_dir`.
    /// `cache_dir` should be a persistent directory (e.g. `%LOCALAPPDATA%/cheime/cache`).
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Load or build fragments for a set of dictionary source files.
    ///
    /// Each file is individually hashed and cached. Only files whose hash
    /// does not match an existing cache entry are re-parsed and compiled.
    /// All fragments are then merged into a single CompiledIndex.
    pub fn load_or_build(
        &self,
        files: &[PathBuf],
        dict_name: &str,
        columns: &[DictColumn],
        generation: DeploymentGeneration,
    ) -> Result<CompiledIndex, CacheError> {
        if files.is_empty() {
            return Err(CacheError::EmptyFileList);
        }

        let mut all_entries: BTreeMap<String, Vec<(String, Option<i64>)>> = BTreeMap::new();
        let mut total_entries = 0usize;
        let mut combined_hash_state = String::new();

        for file in files {
            let (fragment, _was_cached) = self.load_or_build_one(file, dict_name, columns)?;
            combined_hash_state.push_str(&fragment.source_hash);
            total_entries += fragment.total_entries;
            // Merge fragment into accumulator
            for (code, mut entries) in fragment.entries {
                all_entries.entry(code).or_default().append(&mut entries);
            }
        }

        // Re-sort merged groups by weight desc, text asc
        for group in all_entries.values_mut() {
            group.sort_by(|a, b| {
                b.1.unwrap_or(0)
                    .cmp(&a.1.unwrap_or(0))
                    .then_with(|| a.0.cmp(&b.0))
            });
        }

        // Compute combined hash
        let mut hasher = Sha256::new();
        hasher.update(combined_hash_state.as_bytes());
        let source_hash = format!("{:x}", hasher.finalize());

        Ok(CompiledIndex::from_fragment(
            generation,
            source_hash,
            total_entries,
            all_entries,
        ))
    }

    /// Load a single file's fragment from cache, or build + cache it.
    /// Returns (fragment, was_cache_hit).
    fn load_or_build_one(
        &self,
        file: &Path,
        dict_name: &str,
        columns: &[DictColumn],
    ) -> Result<(CacheFragment, bool), CacheError> {
        let content = fs::read_to_string(file)
            .map_err(|e| CacheError::Io(format!("read {}: {e}", file.display())))?;
        let hash = Self::hash_content(&content);

        // Try cache hit
        let cache_path = self.fragment_path(dict_name, &hash);
        if cache_path.exists() {
            let fragment = self.load_fragment(&cache_path)?;
            return Ok((fragment, true));
        }

        // Cache miss — parse and build
        let body = Self::extract_body(&content);
        let entries = parse_body(body, columns)
            .map_err(|e| CacheError::Parse(format!("{}: {e}", file.display())))?;

        let fragment = Self::compile_fragment(entries, &hash);
        self.save_fragment(&cache_path, &fragment)?;
        Ok((fragment, false))
    }

    /// Compute SHA256 of file content.
    fn hash_content(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Extract TSV body from a .dict.yaml file (skip YAML frontmatter).
    /// Handles both LF and CRLF line endings (Windows compatibility).
    fn extract_body(raw: &str) -> &str {
        let sep = "\n---\n";
        let sep_crlf = "\r\n---\r\n";
        let body = if let Some(p) = raw.find(sep_crlf) {
            &raw[p + sep_crlf.len()..]
        } else if let Some(p) = raw.find(sep) {
            &raw[p + sep.len()..]
        } else {
            return raw;
        };
        // Skip past optional "..." YAML end marker (and its line ending)
        for marker in &["\n...\r\n", "\n...\n"] {
            if let Some(q) = body.find(marker) {
                return &body[q + marker.len()..];
            }
        }
        body
    }

    /// Compile parsed entries into a CacheFragment.
    fn compile_fragment(entries: Vec<DictEntry>, hash: &str) -> CacheFragment {
        let total = entries.len();
        let mut grouped: BTreeMap<String, Vec<(String, Option<i64>)>> = BTreeMap::new();
        for e in entries {
            grouped
                .entry(e.code)
                .or_default()
                .push((e.text, e.weight));
        }
        for group in grouped.values_mut() {
            group.sort_by(|a, b| {
                b.1.unwrap_or(0)
                    .cmp(&a.1.unwrap_or(0))
                    .then_with(|| a.0.cmp(&b.0))
            });
        }
        CacheFragment { total_entries: total, source_hash: hash.to_owned(), entries: grouped }
    }

    // ── Path helpers ────────────────────────────────────────────────

    fn fragment_path(&self, dict_name: &str, hash: &str) -> PathBuf {
        let dir = self.cache_dir.join("dicts").join(dict_name);
        dir.join(format!("{hash}.bin"))
    }

    fn load_fragment(&self, path: &Path) -> Result<CacheFragment, CacheError> {
        let mut file = fs::File::open(path)
            .map_err(|e| CacheError::Io(format!("open cache: {e}")))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .map_err(|e| CacheError::Io(format!("read cache: {e}")))?;
        rmp_serde::from_slice(&buf)
            .map_err(|e| CacheError::Deserialize(e.to_string()))
    }

    fn save_fragment(&self, path: &Path, fragment: &CacheFragment) -> Result<(), CacheError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| CacheError::Io(format!("create cache dir: {e}")))?;
        }
        let buf = rmp_serde::to_vec(fragment)
            .map_err(|e| CacheError::Serialize(e.to_string()))?;
        fs::write(path, &buf)
            .map_err(|e| CacheError::Io(format!("write cache: {e}")))?;
        Ok(())
    }

    /// Delete all cached fragments for a dict (force rebuild next time).
    pub fn invalidate(&self, dict_name: &str) -> Result<(), CacheError> {
        let dir = self.cache_dir.join("dicts").join(dict_name);
        if dir.exists() {
            fs::remove_dir_all(&dir)
                .map_err(|e| CacheError::Io(format!("invalidate cache: {e}")))?;
        }
        Ok(())
    }

    /// List cached hashes for a dict (for diagnostics).
    pub fn cached_hashes(&self, dict_name: &str) -> Result<Vec<String>, CacheError> {
        let dir = self.cache_dir.join("dicts").join(dict_name);
        if !dir.exists() { return Ok(vec![]); }
        let mut hashes = Vec::new();
        for entry in fs::read_dir(&dir)
            .map_err(|e| CacheError::Io(format!("list cache: {e}")))?
        {
            let entry = entry.map_err(|e| CacheError::Io(format!("read dir: {e}")))?;
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".bin") {
                    hashes.push(name.trim_end_matches(".bin").to_owned());
                }
            }
        }
        hashes.sort();
        Ok(hashes)
    }
}

// ── CacheError ─────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum CacheError {
    Io(String),
    Parse(String),
    Serialize(String),
    Deserialize(String),
    EmptyFileList,
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(s) => write!(f, "I/O: {s}"),
            Self::Parse(s) => write!(f, "parse: {s}"),
            Self::Serialize(s) => write!(f, "serialize: {s}"),
            Self::Deserialize(s) => write!(f, "deserialize: {s}"),
            Self::EmptyFileList => write!(f, "no dictionary files provided"),
        }
    }
}

impl std::error::Error for CacheError {}

// ── CompiledIndex extension ────────────────────────────────────────

impl CompiledIndex {
    /// Construct from a cache fragment (avoids re-sorting).
    pub(crate) fn from_fragment(
        generation: DeploymentGeneration,
        source_hash: String,
        total_entries: usize,
        entries: BTreeMap<String, Vec<(String, Option<i64>)>>,
    ) -> Self {
        Self { generation, source_hash, total_entries, entries }
    }

    /// Access the internal entries map (for testing only).
    #[cfg(test)]
    pub(crate) fn entries_map(&self) -> &BTreeMap<String, Vec<(String, Option<i64>)>> {
        &self.entries
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DictColumn;
    use tempfile::TempDir;

    fn sample_dict() -> &'static str {
        "好\thao\t100\n你\tni\t200\n你好\tni hao\t300\n拟好\tni hao\t50\n"
    }

    fn sample_file(tmp: &TempDir, name: &str, content: &str) -> PathBuf {
        let path = tmp.path().join(name);
        fs::write(&path, content).unwrap();
        path
    }

    fn columns() -> Vec<DictColumn> {
        vec![DictColumn::Text, DictColumn::Code, DictColumn::Weight]
    }

    #[test]
    fn cache_hit_avoids_rebuild() {
        let tmp = TempDir::new().unwrap();
        let cache = DictCache::new(tmp.path().join("cache"));
        let file = sample_file(&tmp, "test.dict.yaml", sample_dict());

        // First build — cache miss
        let idx1 = cache.load_or_build(&[file.clone()], "test", &columns(), DeploymentGeneration::new(1)).unwrap();
        assert_eq!(idx1.total_entries, 4);

        // Second build — cache hit, same content
        let idx2 = cache.load_or_build(&[file.clone()], "test", &columns(), DeploymentGeneration::new(1)).unwrap();
        assert_eq!(idx2.source_hash, idx1.source_hash);
        assert_eq!(idx2.total_entries, 4);
    }

    #[test]
    fn file_change_invalidates_cache() {
        let tmp = TempDir::new().unwrap();
        let cache = DictCache::new(tmp.path().join("cache"));
        let file = sample_file(&tmp, "test.dict.yaml", sample_dict());

        let idx1 = cache.load_or_build(&[file.clone()], "test", &columns(), DeploymentGeneration::new(1)).unwrap();
        let hash1 = idx1.source_hash.clone();

        // Change file content
        fs::write(&file, "新\txin\t500\n").unwrap();
        let idx2 = cache.load_or_build(&[file.clone()], "test", &columns(), DeploymentGeneration::new(1)).unwrap();
        assert_ne!(idx2.source_hash, hash1, "hash should change when file changes");
        assert_eq!(idx2.total_entries, 1);
    }

    #[test]
    fn split_files_merge_correctly() {
        let tmp = TempDir::new().unwrap();
        let cache = DictCache::new(tmp.path().join("cache"));

        let file_a = sample_file(&tmp, "a.dict.yaml", "好\thao\t100\n你\tni\t200\n");
        let file_b = sample_file(&tmp, "b.dict.yaml", "你好\tni hao\t300\n拟好\tni hao\t50\n");

        let idx = cache.load_or_build(
            &[file_a.clone(), file_b.clone()],
            "split",
            &columns(),
            DeploymentGeneration::new(1),
        ).unwrap();

        assert_eq!(idx.total_entries, 4);
        // Both files' entries should be merged
        let ni_hao = idx.query("ni hao");
        assert_eq!(ni_hao.len(), 2);
        assert_eq!(ni_hao[0].text, "你好"); // weight 300 > 50
    }

    #[test]
    fn partial_rebuild_only_changed_files() {
        let tmp = TempDir::new().unwrap();
        let cache = DictCache::new(tmp.path().join("cache"));

        let file_a = sample_file(&tmp, "a.dict.yaml", "好\thao\t100\n");
        let file_b = sample_file(&tmp, "b.dict.yaml", "你\tni\t200\n");

        // First full build
        let idx1 = cache.load_or_build(
            &[file_a.clone(), file_b.clone()], "partial", &columns(), DeploymentGeneration::new(1),
        ).unwrap();
        assert_eq!(idx1.total_entries, 2);

        // Change only file_b
        fs::write(&file_b, "你\tni\t200\n呢\tne\t150\n").unwrap();

        let idx2 = cache.load_or_build(
            &[file_a.clone(), file_b.clone()], "partial", &columns(), DeploymentGeneration::new(1),
        ).unwrap();
        assert_eq!(idx2.total_entries, 3);
    }

    #[test]
    fn invalidate_clears_cache() {
        let tmp = TempDir::new().unwrap();
        let cache = DictCache::new(tmp.path().join("cache"));
        let file = sample_file(&tmp, "test.dict.yaml", sample_dict());

        cache.load_or_build(&[file.clone()], "test", &columns(), DeploymentGeneration::new(1)).unwrap();
        assert!(!cache.cached_hashes("test").unwrap().is_empty());

        cache.invalidate("test").unwrap();
        assert!(cache.cached_hashes("test").unwrap().is_empty());
    }

    #[test]
    fn cache_loads_correct_entries() {
        let tmp = TempDir::new().unwrap();
        let cache = DictCache::new(tmp.path().join("cache"));
        let file = sample_file(&tmp, "test.dict.yaml", sample_dict());

        let idx = cache.load_or_build(&[file], "test", &columns(), DeploymentGeneration::new(1)).unwrap();

        // Query by code
        let hao = idx.query("hao");
        assert_eq!(hao.len(), 1);
        assert_eq!(hao[0].text, "好");

        let ni_hao = idx.query("ni hao");
        assert_eq!(ni_hao.len(), 2);
        assert!(ni_hao.iter().any(|c| c.text == "你好"));
        assert!(ni_hao.iter().any(|c| c.text == "拟好"));
    }
}
