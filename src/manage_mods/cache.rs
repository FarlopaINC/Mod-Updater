use redb::{Database, TableDefinition, ReadableTable};
use std::path::PathBuf;
use std::fs;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use crate::paths_vars::PATHS;
use crate::manage_mods::models::{ModInfo, CachedFile, CachedProject};

// Define tables
// FILES: filename (String) -> serialized CachedFile (JSON String)
const TABLE_FILES: TableDefinition<&str, &str> = TableDefinition::new("files");
// PROJECTS: project_id (String) -> serialized CachedProject (JSON String)
const TABLE_PROJECTS: TableDefinition<&str, &str> = TableDefinition::new("projects");

// Global Database Instance
static DB: Lazy<Mutex<Option<Database>>> = Lazy::new(|| Mutex::new(None));

pub fn init() {
    let mut db_path = PATHS.base_game_folder.clone();
    
    if let Some(mut path) = dirs::cache_dir() {
        path.push("mods_updater");
        if !path.exists() {
            let _ = std::fs::create_dir_all(&path);
        }
        // Use v2 to avoid conflicts with old schema
        path.push("mods_cache_v2.redb");
        db_path = path;
    } else {
        db_path.push("mods_cache_v2.redb");
    }

    init_with_path(db_path);
}

pub fn init_with_path(db_path: PathBuf) {
    match Database::create(db_path) {
        Ok(db) => {
            // Create tables if not exist
            let write_txn = db.begin_write().unwrap();
            let _ = write_txn.open_table(TABLE_FILES).unwrap();
            let _ = write_txn.open_table(TABLE_PROJECTS).unwrap();
            
            write_txn.commit().unwrap();
            
            let mut lock = DB.lock().unwrap();
            *lock = Some(db);
        },
        Err(e) => println!("Error inicializando Redb: {}", e),
    }
}

pub fn get_mod(filename: &str) -> Option<ModInfo> {
    let lock = DB.lock().unwrap();
    if let Some(db) = lock.as_ref() {
        let read_txn = db.begin_read().ok()?;
        let table_files = read_txn.open_table(TABLE_FILES).ok()?;
        let table_projects = read_txn.open_table(TABLE_PROJECTS).ok()?;
        
        // 1. Get File Info
        if let Some(access) = table_files.get(filename).ok()? {
             let file_json = access.value();
             if let Ok(cached_file) = serde_json::from_str::<CachedFile>(file_json) {
                 
                 // 2. Get Project Info
                 let project_id = &cached_file.project_ref;
                 let cached_project = if let Some(proj_access) = table_projects.get(project_id.as_str()).ok()? {
                     serde_json::from_str::<CachedProject>(proj_access.value()).ok()
                 } else {
                     None
                 };

                 if let Some(proj) = cached_project {
                     // 3. Combine into ModInfo
                     return Some(ModInfo {
                         key: filename.to_string(),
                         name: proj.name,
                         detected_project_id: proj.detected_project_id,
                         confirmed_project_id: proj.confirmed_project_id,
                         version_local: cached_file.version_local,
                         version_remote: proj.version_remote,
                         selected: true, // Always default to true, not persisted
                         file_size_bytes: cached_file.file_size_bytes,
                         file_mtime_secs: cached_file.file_mtime_secs,
                         depends: cached_file.depends,
                     });
                 }
             }
        }
    } else {
        // Auto-init retry logic could go here if needed, but omitted for brevity/safety
    }
    None
}

pub fn upsert_mod(filename: &str, info: &ModInfo) {
    let lock = DB.lock().unwrap();
    if let Some(db) = lock.as_ref() {
        if let Ok(write_txn) = db.begin_write() {
            {
                // We need to write to both tables
                let mut table_files = write_txn.open_table(TABLE_FILES).unwrap();
                let mut table_projects = write_txn.open_table(TABLE_PROJECTS).unwrap();

                // 1. Identify Project ID (Use confirmed if available, else detected/fallback)
                // Ideally we use a stable ID. info.detected_project_id is usually the mod ID from json.
                let project_id = info.confirmed_project_id.clone()
                    .or(info.detected_project_id.clone())
                    .unwrap_or(info.name.clone()); // Fallback to name if absolutely nothing else

                // 2. Upsert Project
                // We merge with existing project info if possible to preserve confirmed_project_id if we only have detected?
                // Actually, ModInfo usually has the latest state we want to save. 
                // BUT, if info.confirmed_project_id is None, we shouldn't overwrite an existing confirmed ID in DB with None.
                
                let mut project_to_save = CachedProject {
                    name: info.name.clone(),
                    detected_project_id: info.detected_project_id.clone(),
                    confirmed_project_id: info.confirmed_project_id.clone(),
                    version_remote: info.version_remote.clone(),
                };

                // Check existing project to preserve "confirmed_id" or "version_remote" if current info is lacking?
                // For now, let's assume 'info' might be fresh from disk (no remote info) OR updated from remote.
                // If it's fresh from disk, confirmed_id is None. We don't want to wipe existing confirmed_id.
                if let Ok(Some(existing_access)) = table_projects.get(project_id.as_str()) {
                    if let Ok(existing) = serde_json::from_str::<CachedProject>(existing_access.value()) {
                        if project_to_save.confirmed_project_id.is_none() {
                            project_to_save.confirmed_project_id = existing.confirmed_project_id;
                        }
                        if project_to_save.version_remote.is_none() {
                             project_to_save.version_remote = existing.version_remote;
                        }
                    }
                }

                if let Ok(json_proj) = serde_json::to_string(&project_to_save) {
                    let _ = table_projects.insert(project_id.as_str(), json_proj.as_str());
                }

                // 3. Upsert File
                let file_to_save = CachedFile {
                    file_size_bytes: info.file_size_bytes,
                    file_mtime_secs: info.file_mtime_secs,
                    version_local: info.version_local.clone(),
                    depends: info.depends.clone(),
                    project_ref: project_id.clone(),
                };

                if let Ok(json_file) = serde_json::to_string(&file_to_save) {
                    let _ = table_files.insert(filename, json_file.as_str());
                }
            }
            let _ = write_txn.commit();
        }
    }
}

pub fn update_remote_info(filename: &str, project_id: Option<String>, version_remote: Option<String>) {
    let lock = DB.lock().unwrap();
    if let Some(db) = lock.as_ref() {
        let write_txn = match db.begin_write() {
            Ok(txn) => txn,
            Err(_) => return,
        };
        
        {
            let table_files = match write_txn.open_table(TABLE_FILES) {
                Ok(t) => t,
                Err(_) => return,
            };
            let mut table_projects = match write_txn.open_table(TABLE_PROJECTS) {
                Ok(t) => t,
                Err(_) => return,
            };

            // 1. Find the project ref for this file
            let project_ref = if let Ok(Some(access)) = table_files.get(filename) {
                if let Ok(file) = serde_json::from_str::<CachedFile>(access.value()) {
                    Some(file.project_ref)
                } else { None }
            } else { None };

            // 2. Update the project
            if let Some(p_ref) = project_ref {
                 let new_json = if let Ok(Some(access)) = table_projects.get(p_ref.as_str()) {
                    if let Ok(mut proj) = serde_json::from_str::<CachedProject>(access.value()) {
                        proj.confirmed_project_id = project_id;
                        proj.version_remote = version_remote;
                        serde_json::to_string(&proj).ok()
                    } else { None }
                 } else { None };

                 if let Some(json) = new_json {
                     let _ = table_projects.insert(p_ref.as_str(), json.as_str());
                 }
            }
        }
        let _ = write_txn.commit();
    }
}

pub fn prune_db(valid_filenames: &std::collections::HashSet<String>) -> usize {
    let lock = DB.lock().unwrap();
    if let Some(db) = lock.as_ref() {
        let write_txn = match db.begin_write() {
            Ok(txn) => txn,
            Err(_) => return 0,
        };
        
        let count;
        {
            let mut table = match write_txn.open_table(TABLE_FILES) {
                Ok(t) => t,
                Err(_) => return 0,
            };

            let mut to_remove = Vec::new();
            if let Ok(iter) = table.iter() {
                for item in iter {
                    if let Ok((k_access, _)) = item {
                        let key = k_access.value();
                        if !valid_filenames.contains(key) {
                            to_remove.push(key.to_string());
                        }
                    }
                }
            }

            count = to_remove.len();
            for k in to_remove {
                let _ = table.remove(k.as_str());
            }
            
            // Note: We could also prune orphaned projects (projects with no files referencing them),
            // but that requires a more expensive full scan. For now, pruning files is the critical part
            // to ensure scanner doesn't pick up ghosts.
        }
        let _ = write_txn.commit();
        return count;
    }
    0
}

pub fn clean_cache() {
    let modpacks_folder = &PATHS.modpacks_folder;
    if !modpacks_folder.exists() {
        return;
    }

    let mut valid_keys = std::collections::HashSet::new();

    if let Ok(entries) = fs::read_dir(modpacks_folder) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                if let Ok(sub_entries) = fs::read_dir(&path) {
                    for sub_entry in sub_entries.filter_map(|e| e.ok()) {
                        let sub_path = sub_entry.path();
                        if sub_path.is_file() && sub_path.extension().and_then(|s| s.to_str()) == Some("jar") {
                             let filename = sub_path.file_name().unwrap_or_default().to_string_lossy().to_string();
                             valid_keys.insert(filename);
                        }
                    }
                }
            }
        }
    }

    let removed_count = prune_db(&valid_keys);

    if removed_count > 0 {
        println!("Limpieza de caché (Redb): eliminadas {} entradas huérfanas.", removed_count);
    } else {
        println!("Limpieza de caché (Redb): todo correcto.");
    }
}
