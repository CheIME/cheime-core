#[cfg(test)]
mod debug_tests {
    use super::*;
    use crate::format::{parse_header, read_code_idx_entry, read_block_entry, read_text, write_tidex};
    use tempfile::NamedTempFile;

    #[test]
    fn dump_file_layout() {
        let ni = vec![("你".to_string(), 200), ("呢".to_string(), 90), ("尼".to_string(), 70)];
        let ni_hao = vec![("你好".to_string(), 300)];
        let ni_men = vec![("你们".to_string(), 150)];
        let na = vec![("那".to_string(), 100), ("拿".to_string(), 60)];
        let za = vec![("咋".to_string(), 50)];
        let code_entries: Vec<(&str, &[(String, i32)])> = vec![
            ("na", &na[..]),
            ("ni", &ni[..]),
            ("ni hao", &ni_hao[..]),
            ("ni men", &ni_men[..]),
            ("za", &za[..]),
        ];

        let tmp = NamedTempFile::new().unwrap();
        write_tidex(tmp.path(), &code_entries).unwrap();

        let data = std::fs::read(tmp.path()).unwrap();
        println!("File size: {} bytes", data.len());

        let hdr = parse_header(&data).unwrap();
        println!("Header: {:?}", hdr);

        // Inspect code index
        for i in 0..hdr.code_count {
            let (code_off, first_blk, count) = read_code_idx_entry(&data, hdr.code_idx_off as usize, i);
            let code_str = unsafe { read_text(&data, code_off) };
            println!("  L1[{}]: code_off={}, first_blk={}, count={}, code='{}'",
                i, code_off, first_blk, count, code_str);
            for j in 0..count {
                let (text_off, weight) = read_block_entry(&data, hdr.block_tbl_off as usize, first_blk + j);
                let text_str = unsafe { read_text(&data, text_off) };
                println!("    L2[{}]: text_off={}, weight={}, text='{}'",
                    first_blk + j, text_off, weight, text_str);
            }
        }
    }
}
