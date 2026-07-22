#![allow(unsafe_code)]

//! Tidex binary index format — sorted, mmap-friendly disk index.
//!
//! Two-level layout:
//!
//! ```text
//! ╔═══════════════════════════════════════════╗
//! ║  Header (64 bytes)                       ║
//! ╠═══════════════════════════════════════════╣
//! ║  Code Index (code_count × 12 bytes)      ║  ← sorted by code string
//! ║    [u32 code_off_le]  → text pool        ║
//! ║    [u32 first_blk_le] → block table idx  ║
//! ║    [u32 count_le]     → entry count      ║
//! ╠═══════════════════════════════════════════╣
//! ║  Block Table (entry_count × 8 bytes)     ║
//! ║    [u32 text_off_le]  → text pool        ║
//! ║    [i32 weight_le]                       ║
//! ╠═══════════════════════════════════════════╣
//! ║  Text Pool                               ║
//! ║    [u16 len_le][u8; len (utf-8)] × N     ║
//! ╚═══════════════════════════════════════════╝
//! ```
//!
//! All multi-byte integers are **little-endian**.
//! Level-1 records are sorted by the code string in the text pool so binary
//! search over the 12-byte fixed-width records finds exact/prefix matches.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufWriter, Seek, Write};
use std::path::Path;

// ---------------------------------------------------------------------------
// Magic & version
// ---------------------------------------------------------------------------

pub const TIDX_MAGIC: &[u8; 4] = b"TIDX";
pub const TIDX_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Header — 64 bytes, zero-padded tail
// ---------------------------------------------------------------------------

pub const HEADER_SIZE: usize = 64;

pub fn pack_header(
    code_count: u32,
    entry_count: u32,
    text_pool_size: u32,
    code_idx_off: u32,
    block_tbl_off: u32,
    text_pool_off: u32,
) -> [u8; HEADER_SIZE] {
    let mut h = [0u8; HEADER_SIZE];
    h[0..4].copy_from_slice(TIDX_MAGIC);
    h[4..8].copy_from_slice(&TIDX_VERSION.to_le_bytes());
    h[8..12].copy_from_slice(&code_count.to_le_bytes());
    h[12..16].copy_from_slice(&entry_count.to_le_bytes());
    h[16..20].copy_from_slice(&text_pool_size.to_le_bytes());
    h[20..24].copy_from_slice(&code_idx_off.to_le_bytes());
    h[24..28].copy_from_slice(&block_tbl_off.to_le_bytes());
    h[28..32].copy_from_slice(&text_pool_off.to_le_bytes());
    h
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TidexHeader {
    pub code_count: u32,
    pub entry_count: u32,
    pub text_pool_size: u32,
    pub code_idx_off: u32,
    pub block_tbl_off: u32,
    pub text_pool_off: u32,
}

pub fn parse_header(data: &[u8]) -> Result<TidexHeader, &'static str> {
    if data.len() < HEADER_SIZE {
        return Err("file too small for header");
    }
    if &data[0..4] != TIDX_MAGIC {
        return Err("bad magic — not a .tidx file");
    }
    let version = u32::from_le_bytes(data[4..8].try_into().unwrap());
    if version != TIDX_VERSION {
        return Err("unsupported version");
    }
    Ok(TidexHeader {
        code_count: u32::from_le_bytes(data[8..12].try_into().unwrap()),
        entry_count: u32::from_le_bytes(data[12..16].try_into().unwrap()),
        text_pool_size: u32::from_le_bytes(data[16..20].try_into().unwrap()),
        code_idx_off: u32::from_le_bytes(data[20..24].try_into().unwrap()),
        block_tbl_off: u32::from_le_bytes(data[24..28].try_into().unwrap()),
        text_pool_off: u32::from_le_bytes(data[28..32].try_into().unwrap()),
    })
}

// ---------------------------------------------------------------------------
// Level-1 code index entry — 12 bytes (fixed width → O(1) binary search)
// ---------------------------------------------------------------------------

#[inline]
pub fn write_code_idx_entry(
    w: &mut impl Write,
    code_off: u32,
    first_blk: u32,
    count: u32,
) -> io::Result<()> {
    w.write_all(&code_off.to_le_bytes())?;
    w.write_all(&first_blk.to_le_bytes())?;
    w.write_all(&count.to_le_bytes())
}

#[inline]
pub fn read_code_idx_entry(data: &[u8], base: usize, idx: u32) -> (u32, u32, u32) {
    let off = base + idx as usize * 12;
    let code_off = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
    let first_blk = u32::from_le_bytes(data[off + 4..off + 8].try_into().unwrap());
    let count = u32::from_le_bytes(data[off + 8..off + 12].try_into().unwrap());
    (code_off, first_blk, count)
}

// ---------------------------------------------------------------------------
// Level-2 block entry — 8 bytes
// ---------------------------------------------------------------------------

#[inline]
pub fn write_block_entry(w: &mut impl Write, text_off: u32, weight: i32) -> io::Result<()> {
    w.write_all(&text_off.to_le_bytes())?;
    w.write_all(&weight.to_le_bytes())
}

#[inline]
pub fn read_block_entry(data: &[u8], base: usize, idx: u32) -> (u32, i32) {
    let off = base + idx as usize * 8;
    let text_off = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
    let weight = i32::from_le_bytes(data[off + 4..off + 8].try_into().unwrap());
    (text_off, weight)
}

// ---------------------------------------------------------------------------
// Text pool helpers
// ---------------------------------------------------------------------------

/// Read a `[u16 len_le][utf8]` string at the given offset.
///
/// # Safety
/// `offset` must point to a valid pool entry within `data`.
#[inline]
pub unsafe fn read_text(data: &[u8], offset: u32) -> &str {
    let off = offset as usize;
    let len = u16::from_le_bytes(data[off..off + 2].try_into().unwrap()) as usize;
    let bytes = &data[off + 2..off + 2 + len];
    unsafe { std::str::from_utf8_unchecked(bytes) }
}

/// Write a `[u16 len_le][utf8]` string. Returns the offset where it was written.
///
/// # Panics
/// Panics if `text` exceeds 65535 bytes (u16::MAX).
#[inline]
pub fn write_text(w: &mut (impl Write + Seek), text: &str) -> io::Result<u32> {
    let pos = w.stream_position()?;
    let bytes = text.as_bytes();
    let len = u16::try_from(bytes.len()).expect("text exceeds u16::MAX bytes");
    w.write_all(&len.to_le_bytes())?;
    w.write_all(bytes)?;
    Ok(pos as u32)
}

/// Compute the byte length of `write_text(text)` — for offset bookkeeping.
#[inline]
pub fn text_written_len(text: &str) -> u32 {
    2 + text.len() as u32
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

/// Build a `.tidx` file from pre-grouped, sorted entries.
///
/// `code_entries`: ordered by code ascending, each with entries sorted by weight descending.
pub fn write_tidex(path: &Path, code_entries: &[(&str, &[(String, i32)])]) -> io::Result<()> {
    // ---- Phase 1: compute all text offsets (no I/O, just bookkeeping) ----
    let mut text_offsets: HashMap<String, u32> = HashMap::new();
    let mut next_off = 0u32;

    let mut code_offsets: Vec<u32> = Vec::with_capacity(code_entries.len());
    for &(code, entries) in code_entries {
        let code_off = *text_offsets.entry(code.to_string()).or_insert_with(|| {
            let off = next_off;
            next_off += text_written_len(code);
            off
        });
        code_offsets.push(code_off);

        for (text, _weight) in entries {
            text_offsets.entry(text.clone()).or_insert_with(|| {
                let off = next_off;
                next_off += text_written_len(text);
                off
            });
        }
    }

    let text_pool_size = next_off;

    // ---- Phase 2: compute code index + block table entries ----
    let code_count = code_entries.len() as u32;
    let code_idx_off = HEADER_SIZE as u32;
    let block_tbl_off = code_idx_off + code_count * 12;
    let entry_count: u32 = code_entries.iter().map(|(_, e)| e.len() as u32).sum();
    let text_pool_off = block_tbl_off + entry_count * 8;

    let mut code_idx_entries: Vec<(u32, u32, u32)> = Vec::with_capacity(code_entries.len());
    let mut block_entries: Vec<(u32, i32)> = Vec::with_capacity(entry_count as usize);

    for i in 0..code_entries.len() {
        let (_code, entries) = code_entries[i];
        let code_off = text_pool_off + code_offsets[i];
        let first_blk = block_entries.len() as u32;
        let count = entries.len() as u32;
        code_idx_entries.push((code_off, first_blk, count));
        for (text, weight) in entries {
            let text_off = text_pool_off + text_offsets[text.as_str()];
            block_entries.push((text_off, *weight));
        }
    }

    // ---- Phase 3: write everything to disk ----
    let mut f = BufWriter::new(File::create(path)?);

    // Header
    let header = pack_header(
        code_count,
        entry_count,
        text_pool_size,
        code_idx_off,
        block_tbl_off,
        text_pool_off,
    );
    f.write_all(&header)?;

    // Code Index
    debug_assert_eq!(
        f.stream_position()?,
        code_idx_off as u64,
        "code_idx_off mismatch"
    );
    for &(co, fb, cnt) in &code_idx_entries {
        write_code_idx_entry(&mut f, co, fb, cnt)?;
    }

    // Block Table
    debug_assert_eq!(
        f.stream_position()?,
        block_tbl_off as u64,
        "block_tbl_off mismatch"
    );
    for &(to, wt) in &block_entries {
        write_block_entry(&mut f, to, wt)?;
    }

    // Text Pool — skip duplicates to match Phase 1 offset bookkeeping
    debug_assert_eq!(
        f.stream_position()?,
        text_pool_off as u64,
        "text_pool_off mismatch"
    );
    let mut written = std::collections::HashSet::new();
    for i in 0..code_entries.len() {
        let (code, entries) = code_entries[i];
        if written.insert(code.to_string()) {
            write_text(&mut f, code)?;
        }
        for (text, _weight) in entries {
            if written.insert(text.clone()) {
                write_text(&mut f, text)?;
            }
        }
    }

    f.flush()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn write_and_read_structure_valid() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path();

        let ni = [("你".to_string(), 100), ("呢".to_string(), 90)];
        let ni_hao = [("你好".to_string(), 100)];
        let na = [("那".to_string(), 100)];
        let code_entries: Vec<(&str, &[(String, i32)])> =
            vec![("ni", &ni[..]), ("ni hao", &ni_hao[..]), ("na", &na[..])];

        write_tidex(path, &code_entries).unwrap();

        let data = std::fs::read(path).unwrap();
        let hdr = parse_header(&data).unwrap();
        assert_eq!(hdr.code_count, 3);
        assert_eq!(hdr.entry_count, 4);
    }

    #[test]
    fn pack_parse_header_roundtrip() {
        let h = pack_header(100, 5000, 20000, 64, 1264, 41264);
        let parsed = parse_header(&h).unwrap();
        assert_eq!(parsed.code_count, 100);
        assert_eq!(parsed.entry_count, 5000);
        assert_eq!(parsed.text_pool_size, 20000);
    }

    #[test]
    fn bad_magic_rejects() {
        let mut h = pack_header(0, 0, 0, 0, 0, 0);
        h[0] = b'X';
        assert!(parse_header(&h).is_err());
    }

    #[test]
    fn bad_version_rejects() {
        let mut h = pack_header(0, 0, 0, 0, 0, 0);
        h[4..8].copy_from_slice(&999u32.to_le_bytes());
        assert!(parse_header(&h).is_err());
    }
}
