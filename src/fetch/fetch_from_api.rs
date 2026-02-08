use reqwest::blocking::get;
use std::path::Path;
use super::modrinth_api;
use super::curseforge_api;
use super::modrinth_api::ModrinthSearchHit;

#[derive(Debug, Clone)]
pub struct ModDownloadInfo {
    pub filename: String,
    pub url: String,
    pub project_id: String,
    pub version_remote: String,
}

/// Intenta encontrar un mod, primero en Modrinth, y si falla, en CurseForge.
pub fn find_mod_download(mod_name: &str, mod_id: Option<&str>, game_version: &str, loader: &str, curseforge_api_key: &str) -> Option<ModDownloadInfo> {
    println!("🔍 Procesando '{}' (ID: {:?}) v{} [{}]...", mod_name, mod_id.unwrap_or("N/A"), game_version, loader);
    
    // Helper para verificar y descargar
    let try_find_version = |hit: &ModrinthSearchHit| -> Option<ModDownloadInfo> {
        if let Some(modrinth_version) = modrinth_api::fetch_modrinth_version(&hit.project_id, game_version, loader) {
            if let Some(file) = modrinth_version.first_file() {
                println!("✅ Encontrado en Modrinth: {} (Project: {})", file.filename, hit.title);
                // Convertimos del tipo `ModFile` a nuestro tipo unificado
                return Some(ModDownloadInfo {
                    filename: file.filename.clone(),
                    url: file.url.clone(),
                    project_id: hit.project_id.clone(),
                    version_remote: game_version.to_string(), // Or specific version ID if we had it, but game_version is what we matched against or what we requested
                });
            }
        }
        return None;
    };

    // --- 1. Intento por ID (Si existe) ---
    if let Some(id) = mod_id {
        let hits = modrinth_api::search_modrinth_project(id, &None, &None, 0, 5);
        for hit in &hits {
            if hit.slug == id {
                if let Some(info) = try_find_version(hit) {
                    return Some(info);
                }
            }
        }
    }

    // --- 2. Intento por Nombre (Fallback) ---
    let hits = modrinth_api::search_modrinth_project(mod_name, &None, &None, 0, 5);
    for hit in &hits {
        let slug_match = mod_id.map_or(false, |id| hit.slug == id);
        let name_match = hit.title.to_lowercase().contains(&mod_name.to_lowercase()) || mod_name.to_lowercase().contains(&hit.title.to_lowercase());

        if slug_match || name_match {
            if let Some(info) = try_find_version(hit) {
                return Some(info);
            }
        }
    }
    
    println!("⚠️ No encontrado en Modrinth. Probando en CurseForge...");

    // --- 3. Intento con CurseForge (Fallback) ---
    if let Some(curseforge_id) = curseforge_api::fetch_curseforge_project_id(mod_name, curseforge_api_key) {
        if let Some(curse_file) = curseforge_api::fetch_curseforge_version_file(curseforge_id, game_version, loader, curseforge_api_key) {
            if let Some(download_url) = curse_file.download_url {
                println!("✅ Encontrado en CurseForge: {}", curse_file.file_name);
                return Some(ModDownloadInfo {
                    filename: curse_file.file_name.clone(),
                    url: download_url,
                    project_id: curseforge_id.to_string(),
                    version_remote: curse_file.file_name, // CurseForge uses filename often as version indicator or we can track it
                });
            } else {
                println!("❌ CurseForge encontró el archivo pero no tiene URL de descarga directa.");
            }
        }
    }

    println!("❌ No se encontró '{}' en ninguna plataforma.", mod_name);
    return None;
}

pub fn download_mod_file(file_url: &str, output_folder: &str, filename: &str) -> Result<(), std::io::Error> {
    let mut resp_file = get(file_url).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    // Use Windows-friendly separators; ensure output folder exists
    let dest_path = format!("{}\\{}", output_folder, filename);
    if let Some(parent) = Path::new(&dest_path).parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut out_file = std::fs::File::create(&dest_path)?;
    std::io::copy(&mut resp_file, &mut out_file)?;
    Ok(())
}