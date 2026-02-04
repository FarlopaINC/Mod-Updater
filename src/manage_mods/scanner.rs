use crate::paths_vars::PATHS;
use super::models::{ModInfo, FabricModJson, VersionManifest};
use indexmap::IndexMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;
use std::time::SystemTime;

fn get_file_mtime(metadata: &fs::Metadata) -> u64 {
    metadata.modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn read_mods_in_folder(mods_folder: &str) -> IndexMap<String, ModInfo> {
    let mut mods_map: IndexMap<String, ModInfo> = IndexMap::new();
    let mut cache = load_cache();
    let mut cache_dirty = false;

    if let Ok(entries) = fs::read_dir(mods_folder) {
        let mut entries_vec: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        // Sort by filename to ensure consistent order
        entries_vec.sort_by_key(|e| e.file_name());

        for entry in entries_vec {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("jar") {
                let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                
                // Get Metadata
                let (file_size, file_mtime) = if let Ok(meta) = fs::metadata(&path) {
                    (meta.len(), get_file_mtime(&meta))
                } else {
                    (0, 0)
                };

                // Check Cache
                let mut use_cache = false;
                if let Some(cached) = cache.get(&filename) {
                     // Verify integrity
                    if cached.file_size_bytes == Some(file_size) && cached.file_mtime_secs == Some(file_mtime) {
                        mods_map.insert(cached.key.clone(), cached.clone());
                        use_cache = true;
                    }
                }

                if !use_cache {
                    if let Ok(mut mod_info) = read_single_mod(&path) {
                        // Update metadata
                        mod_info.file_size_bytes = Some(file_size);
                        mod_info.file_mtime_secs = Some(file_mtime);
                        
                        // If previous cache had useful remote info, preserve it if key matches
                        if let Some(old) = cache.get(&filename) {
                            if old.key == mod_info.key {
                                mod_info.confirmed_project_id = old.confirmed_project_id.clone();
                                mod_info.version_remote = old.version_remote.clone();
                                mod_info.selected = old.selected;
                            }
                        }

                        // Update output and cache
                        mods_map.insert(mod_info.key.clone(), mod_info.clone());
                        
                        // We use filename as key in cache to quickly lookup by file
                        cache.insert(filename.clone(), mod_info); 
                        cache_dirty = true;
                    } 
                }
            }        
        }
    }
    
    if cache_dirty {
        save_cache(&cache);
    }
    
    return mods_map;
}

pub fn read_single_mod(path: &Path) -> Result<ModInfo, String> {
    let file = File::open(path).map_err(|_| "No se pudo abrir archivo".to_string())?;
    
    let mut zip = ZipArchive::new(file).map_err(|_| "No es un ZIP válido".to_string())?;

    // Buscamos fabric.mod.json dentro del .jar
    let mut mod_json_str = String::new();
    let mut found = false;
    for i in 0..zip.len() {
        if let Ok(mut file) = zip.by_index(i) {
            if Path::new(file.name()).file_name().and_then(|s| s.to_str()) == Some("fabric.mod.json") {
                if file.read_to_string(&mut mod_json_str).is_ok() {
                    found = true;
                    break;
                }
            }
        }
    }

    if !found || mod_json_str.is_empty() {
        return Err("No se encontró fabric.mod.json".to_string());
    }

    let mod_json: FabricModJson = serde_json::from_str(&mod_json_str)
        .map_err(|_| "Error parseando fabric.mod.json".to_string())?;

    let key = path.file_name().and_then(|s| s.to_str()).unwrap_or(&mod_json.name).to_string();

    let depends = mod_json.depends.map(|deps| {
        deps.into_iter().filter_map(|(k, v)| {
            match v {
                serde_json::Value::String(s) => Some((k, s)),
                serde_json::Value::Array(arr) => {
                     let s = arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                        .join(" || "); // approximation
                     if s.is_empty() { None } else { Some((k, s)) }
                },
                _ => None,
            }
        }).collect()
    });

    Ok(ModInfo {
        key: key.clone(),
        name: mod_json.name,
        detected_project_id: Some(mod_json.id),
        confirmed_project_id: None,
        version_local: mod_json.version,
        version_remote: None,
        selected: true,
        file_size_bytes: None,
        file_mtime_secs: None,
        depends,
    })
}

fn cache_path() -> Option<std::path::PathBuf> {
    if let Some(mut dir) = dirs::cache_dir() {
        dir.push("mods_updater");
        if !dir.exists() {
            let _ = fs::create_dir_all(&dir);
        }
        dir.push("modrinth_cache.json");
        return Some(dir);
    }
    return None;
}

pub fn load_cache() -> IndexMap<String, ModInfo> {
    if let Some(path) = cache_path() {
        if path.exists() {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(map) = serde_json::from_str::<IndexMap<String, ModInfo>>(&data) {
                    return map;
                }
            }
        }
    }
    return IndexMap::new();
}

pub fn save_cache(map: &IndexMap<String, ModInfo>) {
    if let Some(path) = cache_path() {
        if let Ok(data) = serde_json::to_string_pretty(map) {
            let _ = fs::write(path, data);
        }
    }
}

pub fn get_minecraft_versions(manifest_path: &str) -> Vec<String> {
    let data = fs::read_to_string(manifest_path)
        .expect("No se pudo leer version_manifest_v2.json");
    let manifest: VersionManifest =
        serde_json::from_str(&data).expect("Error al parsear el JSON");
    
    return manifest
        .versions
        .into_iter()
        .filter(|v| v.version_type == "release")
        .map(|v| v.id)
        .collect();
}

pub fn list_modpacks() -> Vec<String> {
    let modpacks_folder = &PATHS.modpacks_folder;
    if !modpacks_folder.exists() {
        return vec![];
    }

    let entries = fs::read_dir(modpacks_folder)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().unwrap().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();

    let mut sorted = entries;
    sorted.sort();
    return sorted;
}
