use crate::paths_vars::PATHS;

use std::fs;
use indexmap::IndexMap;
use serde::{Serialize, Deserialize};
use std::fs::File;
use zip::ZipArchive;
use std::io::Read;
use std::path::Path;

#[cfg(target_family = "unix")]
use std::os::unix::fs::symlink as symlink;

#[cfg(target_family = "windows")]
use std::os::windows::fs::symlink_dir as symlink;


// Información básica de un mod
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModInfo {
    pub key: String,
    pub name: String,
    pub detected_project_id: Option<String>,
    pub confirmed_project_id: Option<String>,
    pub version_local: Option<String>,
    pub version_remote: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Deserialize)]
struct FabricModJson {
    id: String,
    name: String,
}

/// Estructuras para parsear el manifest de versiones de Minecraft
#[derive(Deserialize, Debug)]
struct VersionInfo {
    id: String,
    #[serde(rename = "type")]
    version_type: String,
}

#[derive(Deserialize, Debug)]
struct VersionManifest {
    versions: Vec<VersionInfo>,
}


pub fn read_mods_in_folder (mods_folder: &str) -> IndexMap<String, ModInfo> {
    let mut mods_map: IndexMap<String, ModInfo> = IndexMap::new();

    for entry in fs::read_dir(mods_folder).expect("No se pudo leer la carpeta de mods") {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("jar") {
                let file = File::open(&path).expect("No se pudo abrir el archivo .jar");
                let mut zip = ZipArchive::new(file).expect("No se pudo leer el archivo .jar como zip");

                // Buscamos fabric.mod.json dentro del .jar
                let mut mod_json_str = String::new();
                for i in 0..zip.len() {
                    let mut file = zip.by_index(i).expect("No se pudo acceder al archivo dentro del zip");
                    if file.name().ends_with("fabric.mod.json") {
                        file.read_to_string(&mut mod_json_str).expect("No se pudo leer fabric.mod.json");
                        break;
                    }
                }

                if mod_json_str.is_empty() {
                    println!("❌ No se encontró fabric.mod.json en {:?}", path);
                    continue;
                }

                let mod_json: FabricModJson = serde_json::from_str(&mod_json_str).expect("Error parseando fabric.mod.json");

                let key = path.file_name().and_then(|s| s.to_str()).unwrap_or(&mod_json.name).to_string();

                mods_map.insert(key.clone(), ModInfo {
                    key: key.clone(),
                    name: mod_json.name,
                    detected_project_id: Some(mod_json.id),
                    confirmed_project_id: None,
                    version_local: None,
                    version_remote: None,
                    selected: true,
                }); 
            }        
        }
    }
    return mods_map;
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

pub fn prepare_output_folder(version: &str) {
    let base_path = PATHS.modpacks_folder.to_string_lossy().to_string();
    // Crear carpeta base si no existe
    if !Path::new(&base_path).exists() {
        fs::create_dir_all(&base_path).expect("ERROR: No se pudo crear la carpeta modpacks");
    }
    
    let output_folder = format!("{}/mods{}", base_path, version);
    if !Path::new(&output_folder).exists() {
        fs::create_dir(&output_folder).expect("ERROR: No se pudo crear la carpeta de versión");
    } 
}   

/* 
SELECTOR
*/

pub fn list_modpacks() -> Vec<String> {
    let modpacks_folder = &PATHS.modpacks_folder;
    if !modpacks_folder.exists() {
        return vec![];
    }

    let entries = fs::read_dir(modpacks_folder)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().unwrap().is_dir())
        .filter(|entry| entry.file_name().to_str().unwrap().starts_with("mods"))
        .map(|entry| entry.file_name().to_str().unwrap().to_string())
        .collect::<Vec<_>>();

    let mut sorted = entries;
    sorted.sort();
    return sorted;
}

pub fn change_mods(modpack: &str) {
    // Si ya existe un enlace simbólico, lo eliminamos
    if let Ok(metadata) = std::fs::symlink_metadata(&PATHS.mods_folder) {
        if metadata.file_type().is_symlink() {
            if let Err(e) = std::fs::remove_file(&PATHS.mods_folder) {
                println!("Error al eliminar el enlace anterior: {}", e);
                return;
            }
        } else {
            println!("Advertencia: '{}' no es un enlace simbólico, no se eliminará.", &PATHS.mods_folder.display());
            return;
        }
    }

    match symlink(&PATHS.modpacks_folder.join(modpack), &PATHS.mods_folder) { // source, target
        Ok(_) => {
            println!("Mods cambiados a: {}", modpack);
        }
        Err(e) => {
            println!("Error al cambiar los mods: {}", e);
        }
    }
}