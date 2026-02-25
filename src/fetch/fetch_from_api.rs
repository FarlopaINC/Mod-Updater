use reqwest::blocking::Client;
use std::collections::HashSet;
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
pub struct UnifiedDependency {
    pub mod_id: String,
    // Source identifier could be added here if needed, but for now we'll just store the ID
}

#[derive(Debug, Clone)]
pub struct ModDownloadInfo {
    pub filename: String,
    pub name: String,   // Human-readable project name (title)
    pub slug: String,   // Modrinth slug (same as detected_project_id for scanned mods)
    pub url: String,
    pub project_id: String,
    pub version_remote: String,
    pub dependencies: Vec<UnifiedDependency>,
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
                    name: hit.title.clone(),
                    slug: hit.slug.clone(),
                    url: file.url.clone(),
                    project_id: hit.project_id.clone(),
                    version_remote: game_version.to_string(),
                    dependencies: modrinth_version.required_deps(),
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
                
                let (title, slug) = modrinth_api::fetch_modrinth_project_info(id)
                    .unwrap_or_else(|| (file.filename.clone(), id.to_string()));
                return Some(ModDownloadInfo {
                    filename: file.filename.clone(),
                    name: title,
                    slug,
                    url: file.url.clone(),
                    project_id: id.to_string(),
                    version_remote: game_version.to_string(),
                    dependencies: modrinth_version.required_deps(),
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
                
                let dependencies = curse_file.dependencies.as_ref()
                    .map(|deps| {
                        deps.iter()
                            .filter(|d| d.relation_type == 3) // 3 = requiredDependency
                            .map(|d| UnifiedDependency { mod_id: d.mod_id.to_string() })
                            .collect()
                    })
                    .unwrap_or_default();

                return Some(ModDownloadInfo {
                    filename: curse_file.file_name.clone(),
                    name: curse_file.file_name.clone(),
                    slug: String::new(), // CurseForge has no Modrinth slug
                    url: download_url,
                    project_id: curseforge_id.to_string(),
                    version_remote: curse_file.file_name,
                    dependencies,
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

/// Transitively resolves all **required** dependencies of `root_project_id`.
/// Uses BFS. Skips IDs in `already_installed` and detects cycles via `visited`.
/// Returns a flat list of download infos for every NEW dependency found.
pub fn resolve_all_dependencies(
    root_project_id: &str,
    game_version: &str,
    loader: &str,
    cf_key: &str,
    already_installed: &HashSet<String>,
) -> Vec<ModDownloadInfo> {
    let mut results: Vec<ModDownloadInfo> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    // Seed the queue with the root mod's direct dependencies
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();

    // Mark root as visited so we don't download it as its own dep
    visited.insert(root_project_id.to_string());
    // Also skip anything already on disk
    for id in already_installed {
        visited.insert(id.clone());
    }

    // Fetch the root's deps
    if let Some(root_info) = find_mod_download("", Some(root_project_id), game_version, loader, cf_key) {
        for dep in root_info.dependencies {
            if !visited.contains(&dep.mod_id) {
                queue.push_back(dep.mod_id);
            }
        }
    }

    while let Some(dep_id) = queue.pop_front() {
        if visited.contains(&dep_id) {
            continue;
        }
        visited.insert(dep_id.clone());

        if let Some(info) = find_mod_download("", Some(&dep_id), game_version, loader, cf_key) {
            // Enqueue transitive deps
            for transitive in &info.dependencies {
                if !visited.contains(&transitive.mod_id) {
                    queue.push_back(transitive.mod_id.clone());
                }
            }
            results.push(info);
        } else {
            println!("⚠️  No se pudo resolver la dependencia transitiva: {}", dep_id);
        }
    }

    results
}

/// Obtiene los nombres legibles de las dependencias **requeridas directas** de un proyecto.
/// Versión ligera pensada para mostrar info en la UI de búsqueda sin descargar nada.
/// Intenta Modrinth primero (por project_id), luego CurseForge si falla.
pub fn fetch_dependency_names(
    project_id: &str,
    game_version: &str,
    loader: &str,
    cf_key: &str,
) -> Vec<String> {
    // 1. Try Modrinth
    if let Some(ver) = modrinth_api::fetch_modrinth_version(project_id, game_version, loader) {
        let deps = ver.required_deps();
        if !deps.is_empty() {
            return deps.iter()
                .filter_map(|d| {
                    modrinth_api::fetch_modrinth_project_info(&d.mod_id)
                        .map(|(title, _slug)| title)
                        .or_else(|| Some(d.mod_id.clone()))
                })
                .collect();
        }
        // Version found but no deps → return empty
        return Vec::new();
    }

    // 2. Try CurseForge (project_id might be a numeric CF id)
    if !cf_key.is_empty() {
        if let Ok(cf_id) = project_id.parse::<u32>() {
            if let Some(cf_file) = curseforge_api::fetch_curseforge_version_file(cf_id, game_version, loader, cf_key) {
                let deps: Vec<String> = cf_file.dependencies.as_ref()
                    .map(|deps| {
                        deps.iter()
                            .filter(|d| d.relation_type == 3) // requiredDependency
                            .map(|d| format!("CF:{}", d.mod_id))
                            .collect()
                    })
                    .unwrap_or_default();
                return deps;
            }
        }
    }

    Vec::new()
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