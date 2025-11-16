use reqwest::blocking::get;
use serde::Deserialize;
use reqwest::blocking::Client;
use reqwest::header;

pub fn download_mod_file(file_url: &str, output_folder: &str, filename: &str) -> Result<(), std::io::Error> {
    let mut resp_file = get(file_url).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    let dest_path = format!("{}/{}", output_folder, filename);
    let mut out_file = std::fs::File::create(&dest_path)?;
    std::io::copy(&mut resp_file, &mut out_file)?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ModDownloadInfo {
    pub filename: String,
    pub url: String,
}

/// Intenta encontrar un mod, primero en Modrinth, y si falla, en CurseForge.
pub fn find_mod_download(mod_name: &str, game_version: &str, curseforge_api_key: &str) -> Option<ModDownloadInfo> {
    println!("🔍 Buscando '{}' v{} en Modrinth...", mod_name, game_version);
    
    // --- 1. Intento con Modrinth ---
    if let Some(modrinth_id) = fetch_modrinth_project_id(mod_name) {
        if let Some(modrinth_version) = fetch_modrinth_version(&modrinth_id, game_version) {
            if let Some(file) = modrinth_version.first_file() {
                println!("✅ Encontrado en Modrinth: {}", file.filename);
                // Convertimos del tipo `ModFile` a nuestro tipo unificado
                return Some(ModDownloadInfo {
                    filename: file.filename.clone(),
                    url: file.url.clone(),
                });
            }
        }
    }
    
    println!("⚠️ No encontrado en Modrinth. Probando en CurseForge...");

    // --- 2. Intento con CurseForge (Fallback) ---
    if let Some(curseforge_id) = fetch_curseforge_project_id(mod_name, curseforge_api_key) {
        if let Some(curse_file) = fetch_curseforge_version_file(curseforge_id, game_version, curseforge_api_key) {
            
            // La API de CurseForge a veces devuelve `null` en la URL.
            if let Some(download_url) = curse_file.download_url {
                println!("✅ Encontrado en CurseForge: {}", curse_file.file_name);
                // Convertimos del tipo `CurseFile` a nuestro tipo unificado
                return Some(ModDownloadInfo {
                    filename: curse_file.file_name,
                    url: download_url,
                });
            } else {
                 println!("❌ CurseForge encontró el archivo pero no tiene URL de descarga directa.");
            }
        }
    }

    println!("❌ No se encontró '{}' en ninguna plataforma.", mod_name);
    return None;
}

// ---- MODULO MODRINTH ----

#[derive(Debug, Deserialize)]
pub struct ModrinthVersion {
    files: Vec<ModFile>,
    game_versions: Vec<String>,
    loaders: Vec<String>,
}

impl ModrinthVersion {
    pub fn first_file(&self) -> Option<&ModFile> {
        self.files.first()
    }
}

#[derive(Debug, Deserialize)]
pub struct ModFile {
    pub url: String,
    pub filename: String,
}

#[derive(Debug, Deserialize)]
struct ModrinthSearchResults {
    hits: Vec<ModrinthSearchHit>,
}

#[derive(Debug, Deserialize)]
struct ModrinthSearchHit {
    project_id: String,
}

pub fn fetch_modrinth_project_id(mod_name: &str) -> Option<String> {
    let search_url = format!("https://api.modrinth.com/v2/search?query={}", mod_name);
    match get(&search_url) {
        Ok(resp) => {
            if resp.status().is_success() {
                let results: ModrinthSearchResults = resp.json().unwrap_or(ModrinthSearchResults { hits: vec![] });
                if let Some(hit) = results.hits.first() {
                    return Some(hit.project_id.clone());
                }
            } else {
                println!("❌ Error al buscar '{}' en la API (status {})", mod_name, resp.status());
            }
        }
        Err(e) => {
            println!("❌ Error de conexión al buscar '{}': {}", mod_name, e);
        }
    }
    return None;
}

pub fn fetch_modrinth_version(mod_id: &str, version: &str) -> Option<ModrinthVersion> {
    let api_url = format!("https://api.modrinth.com/v2/project/{}/version", mod_id);
    match get(&api_url) {
        Ok(resp) => {
            if resp.status().is_success() {
                let versions: Vec<ModrinthVersion> = resp.json().unwrap_or_default();
                let matching_version = versions.into_iter().find(|v| {
                    v.loaders.iter().any(|l| l == "fabric") && 
                    v.game_versions.iter().any(|gv| gv == version)
                });
                return matching_version;
            } else {
                println!("❌ Error en API para {}: status {}", mod_id, resp.status());
                None
            }
        }
        Err(e) => {
            println!("❌ Error consultando API de Modrinth: {}", e);
            return None;
        }
    }
}


// ---- MODULO CURSEFORGE ----


#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    data: T,
}

#[derive(Debug, Deserialize)]
struct CurseMod {
    id: u32,
    slug: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseFile {
    pub id: u32,
    pub file_name: String,
    pub download_url: Option<String>,
    pub game_versions: Vec<String>,
    pub mod_loaders: Vec<String>,
}

fn build_client(api_key: &str) -> Client {
    let mut headers = header::HeaderMap::new();
    headers.insert("x-api-key", header::HeaderValue::from_str(api_key).unwrap());
    Client::builder()
        .default_headers(headers)
        .build()
        .unwrap()
}

pub fn fetch_curseforge_project_id(mod_name: &str, api_key: &str) -> Option<u32> {
    let client = build_client(api_key);
    let search_url = format!("https://api.curseforge.com/v1/mods/search?gameId=432&searchFilter={}", mod_name);

    match client.get(&search_url).send() {
        Ok(resp) => {
            if resp.status().is_success() {
                let results: ApiResponse<Vec<CurseMod>> = resp.json().unwrap();
                results.data.first().map(|mod_info| mod_info.id)
            } else {
                println!("❌ Error al buscar '{}' en CurseAPI (status {})", mod_name, resp.status());
                None
            }
        }
        Err(e) => {
            println!("❌ Error de conexión al buscar en CurseForge: {}", e);
            None
        }
    }
}

pub fn fetch_curseforge_version_file(mod_id: u32, game_version: &str, api_key: &str) -> Option<CurseFile> {
    let client = build_client(api_key);
    let api_url = format!("https://api.curseforge.com/v1/mods/{}/files?gameVersion={}&modLoaderType=4", mod_id, game_version);

    match client.get(&api_url).send() {
        Ok(resp) => {
            if resp.status().is_success() {
                let mut files: ApiResponse<Vec<CurseFile>> = resp.json().unwrap();
                if files.data.is_empty() {
                    None
                } else {
                    Some(files.data.remove(0))
                }
            } else {
                println!("❌ Error en API CurseForge para {}: status {}", mod_id, resp.status());
                None
            }
        }
        Err(e) => {
            println!("❌ Error consultando API de CurseForge: {}", e);
            None
        }
    }
}
                