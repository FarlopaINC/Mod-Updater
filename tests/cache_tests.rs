
#[cfg(test)]
mod tests {
    use mods_updater::local_mods_ops::cache::{upsert_mod, get_mod, update_remote_info, prune_db, init_with_path};
    use mods_updater::local_mods_ops::ModInfo;
    use std::fs;

    fn setup_test_db(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("test_mods_updater_integ_{}", name));
        if path.exists() {
            let _ = fs::remove_dir_all(&path);
        }
        let _ = fs::create_dir_all(&path);
        let db_path = path.join("mods_cache.redb");

        // Force initialize global DB with this path via public API
        init_with_path(db_path);
        
        return path;
    }

    #[test]
    fn test_cache_lifecycle_integration() {
        // Just one big test to avoid race conditions on the global DB static
        let _path = setup_test_db("lifecycle_integration");

        let filename = "test_mod_integ.jar";
        let mut info = ModInfo::default();
        info.key = filename.to_string();
        info.name = "Test Mod Integeration".to_string();

        // 1. Upsert
        upsert_mod(filename, &info);

        // 2. Get
        let retrieved = get_mod(filename).expect("Should find mod");
        assert_eq!(retrieved.name, "Test Mod Integeration");

        // 3. Update Remote Info
        update_remote_info(filename, Some("proj_999".to_string()), Some("v2.0".to_string()));
        let updated = get_mod(filename).expect("Should find mod again");
        assert_eq!(updated.confirmed_project_id, Some("proj_999".to_string()));
        assert_eq!(updated.version_remote, Some("v2.0".to_string()));

        // 4. Prune (Clean Cache)
        // Case A: File is valid, should NOT remove
        let mut valid_keys = std::collections::HashSet::new();
        valid_keys.insert(filename.to_string());
        
        let removed = prune_db(&valid_keys);
        assert_eq!(removed, 0);
        assert!(get_mod(filename).is_some());

        // Case B: File is gone, SHOULD remove
        valid_keys.clear();
        let removed_2 = prune_db(&valid_keys);
        assert_eq!(removed_2, 1);
        assert!(get_mod(filename).is_none());
    }
}
