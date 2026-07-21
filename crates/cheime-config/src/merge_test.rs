
#[cfg(test)]
mod t {
    use crate::merge::merge_configs;
    use crate::schema::SchemaConfig;
    
    #[test]
    fn merge_overrides_menu_page_size() {
        let base: SchemaConfig = serde_yaml::from_str(
            "schema_version: 1\nengine: {}\nmenu:\n  page_size: 9\n"
        ).unwrap();
        let patch: SchemaConfig = serde_yaml::from_str(
            "schema_version: 1\nengine: {}\nmenu:\n  page_size: 5\n"
        ).unwrap();
        eprintln!("base.page_size={}", base.menu.page_size);
        eprintln!("patch.page_size={}", patch.menu.page_size);
        let merged = merge_configs(base, patch);
        eprintln!("merged.page_size={}", merged.menu.page_size);
        assert_eq!(merged.menu.page_size, 5);
    }
}
