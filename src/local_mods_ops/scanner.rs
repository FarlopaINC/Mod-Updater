use crate::paths_vars::PATHS;
use super::models::{ModInfo, VersionManifest};
use indexmap::IndexMap;
use std::fs::{self, File};
use std::path::Path;
use zip::ZipArchive;
use std::time::SystemTime;
use super::parsers;

pub fn get_file_mtime(metadata: &fs::Metadata) -> u64 {
    metadata.modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn read_mods_in_folder(mods_folder: &str) -> IndexMap<String, ModInfo> {
    let mut mods_map: IndexMap<String, ModInfo> = IndexMap::new();

    if let Ok(entries) = fs::read_dir(mods_folder) {
        let mut entries_vec: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries_vec.sort_by_key(|e| e.file_name());

        for entry in entries_vec {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("jar") {
                let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                
                let (file_size, file_mtime) = if let Ok(meta) = fs::metadata(&path) {
                    (meta.len(), get_file_mtime(&meta))
                } else {
                    (0, 0)
                };

                let cached = crate::local_mods_ops::cache::get_mod(&filename);
                let mut use_cache = false;
                if let Some(ref c) = cached {
                    if c.file_size_bytes == Some(file_size) && c.file_mtime_secs == Some(file_mtime) {
                        mods_map.insert(c.key.clone(), c.clone());
                        use_cache = true;
                    }
                }

                if !use_cache {
                    if let Ok(mut mod_info) = read_single_mod(&path) {
                        mod_info.file_size_bytes = Some(file_size);
                        mod_info.file_mtime_secs = Some(file_mtime);
                        
                        if let Some(old) = &cached {
                            if old.key == mod_info.key {
                                mod_info.confirmed_project_id = old.confirmed_project_id.clone();
                                mod_info.version_remote = old.version_remote.clone();
                                mod_info.selected = old.selected;
                            }
                        }

                        mods_map.insert(mod_info.key.clone(), mod_info.clone());
                        crate::local_mods_ops::cache::upsert_mod(&filename, &mod_info); 
                    } 
                }
            }        
        }
    } 
    return mods_map;
}

pub fn read_single_mod(path: &Path) -> Result<ModInfo, String> {
    let file = File::open(path).map_err(|_| "No se pudo abrir archivo".to_string())?;
    let mut zip = ZipArchive::new(file).map_err(|_| "No es un ZIP válido".to_string())?;

    parsers::try_all(&mut zip, path)
        .ok_or_else(|| "No se encontró metadata del mod (ni fabric.mod.json ni META-INF/mods.toml)".to_string())
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

    let mut entries: Vec<String> = fs::read_dir(modpacks_folder)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    entries.sort();
    entries
}
