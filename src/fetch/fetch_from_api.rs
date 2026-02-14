use reqwest::blocking::Client;
use std::path::Path;
use once_cell::sync::Lazy;
use super::modrinth_api;
use super::curseforge_api;
use super::modrinth_api::ModrinthSearchHit;

static DOWNLOAD_CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .unwrap_or_default()
});

#[derive(Debug, Clone)]
pub struct ModDownloadInfo {
    pub filename: String,
    pub url: String,
    pub project_id: String,
    pub version_remote: String,
}

/// Intenta resolver un mod en Modrinth (por ID directo, búsqueda por ID, y búsqueda por nombre).
fn try_modrinth(mod_name: &str, mod_id: Option<&str>, game_version: &str, loader: &str) -> Option<ModDownloadInfo> {
    // Helper para verificar y extraer info de un hit de Modrinth
    let try_find_version = |hit: &ModrinthSearchHit| -> Option<ModDownloadInfo> {
        if let Some(modrinth_version) = modrinth_api::fetch_modrinth_version(&hit.project_id, game_version, loader) {
            if let Some(file) = modrinth_version.first_file() {
                println!("✅ Encontrado en Modrinth: {} (Project: {})", file.filename, hit.title);
                return Some(ModDownloadInfo {
                    filename: file.filename.clone(),
                    url: file.url.clone(),
                    project_id: hit.project_id.clone(),
                    version_remote: game_version.to_string(),
                });
            }
        }
        None
    };

    // 1. Intento por ID directo
    if let Some(id) = mod_id {
        if let Some(modrinth_version) = modrinth_api::fetch_modrinth_version(id, game_version, loader) {
            if let Some(file) = modrinth_version.first_file() {
                println!("✅ Encontrado en Modrinth (Directo): {} (ID: {})", file.filename, id);
                return Some(ModDownloadInfo {
                    filename: file.filename.clone(),
                    url: file.url.clone(),
                    project_id: id.to_string(),
                    version_remote: game_version.to_string(),
                });
            }
        }

        // Fallback: búsqueda por slug
        let hits = modrinth_api::search_modrinth_project(id, &None, &None, 0, 5);
        for hit in &hits {
            if hit.slug == id {
                if let Some(info) = try_find_version(hit) {
                    return Some(info);
                }
            }
        }
    }

    // 2. Intento por nombre
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

    None
}

/// Intenta resolver un mod en CurseForge (búsqueda por nombre → fichero de versión).
fn try_curseforge(mod_name: &str, game_version: &str, loader: &str, curseforge_api_key: &str) -> Option<ModDownloadInfo> {
    if curseforge_api_key.is_empty() {
        return None;
    }

    if let Some(curseforge_id) = curseforge_api::fetch_curseforge_project_id(mod_name, curseforge_api_key) {
        if let Some(curse_file) = curseforge_api::fetch_curseforge_version_file(curseforge_id, game_version, loader, curseforge_api_key) {
            if let Some(download_url) = curse_file.download_url {
                println!("✅ Encontrado en CurseForge: {}", curse_file.file_name);
                return Some(ModDownloadInfo {
                    filename: curse_file.file_name.clone(),
                    url: download_url,
                    project_id: curseforge_id.to_string(),
                    version_remote: curse_file.file_name,
                });
            } else {
                println!("❌ CurseForge encontró el archivo pero no tiene URL de descarga directa.");
            }
        }
    }

    None
}

/// Intenta encontrar un mod usando balanceo dinámico entre Modrinth y CurseForge.
/// Ambas APIs se intentan SIEMPRE antes de reportar error — el balanceo solo cambia el orden.
pub fn find_mod_download(mod_name: &str, mod_id: Option<&str>, game_version: &str, loader: &str, curseforge_api_key: &str) -> Option<ModDownloadInfo> {
    println!("🔍 Procesando '{}' (ID: {:?}) v{} [{}]...", mod_name, mod_id.unwrap_or("N/A"), game_version, loader);

    // Decidir orden según capacidad disponible
    let modrinth_first = modrinth_api::has_capacity() || !curseforge_api::is_available();

    if modrinth_first {
        // Orden normal: Modrinth → CurseForge
        if let Some(info) = try_modrinth(mod_name, mod_id, game_version, loader) {
            return Some(info);
        }
        println!("⚠️ No encontrado en Modrinth. Probando en CurseForge...");
        if let Some(info) = try_curseforge(mod_name, game_version, loader, curseforge_api_key) {
            return Some(info);
        }
    } else {
        // Swap: CurseForge primero (Modrinth agotado)
        println!("🔄 Modrinth rate limit bajo, priorizando CurseForge...");
        if let Some(info) = try_curseforge(mod_name, game_version, loader, curseforge_api_key) {
            return Some(info);
        }
        println!("⚠️ No encontrado en CurseForge. Probando en Modrinth...");
        if let Some(info) = try_modrinth(mod_name, mod_id, game_version, loader) {
            return Some(info);
        }
    }

    println!("❌ No se encontró '{}' en ninguna plataforma.", mod_name);
    None
}

pub fn download_mod_file(file_url: &str, output_folder: &str, filename: &str) -> Result<(), std::io::Error> {
    let mut resp_file = DOWNLOAD_CLIENT.get(file_url).send()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    let dest_path = Path::new(output_folder).join(filename);
    let part_path = Path::new(output_folder).join(format!("{}.part", filename));

    if let Some(parent) = dest_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    // Descargar a archivo temporal .part
    let result = {
        let mut out_file = std::fs::File::create(&part_path)?;
        std::io::copy(&mut resp_file, &mut out_file)
    };

    match result {
        Ok(_) => {
            // Descarga completa: renombrar .part -> archivo final
            std::fs::rename(&part_path, &dest_path)?;
            Ok(())
        }
        Err(e) => {
            // Fallo: eliminar el .part corrupto
            let _ = std::fs::remove_file(&part_path);
            Err(e)
        }
    }
}