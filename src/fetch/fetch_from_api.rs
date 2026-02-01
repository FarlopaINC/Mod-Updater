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
pub fn find_mod_download(mod_name: &str, mod_id: Option<&str>, game_version: &str, loader: &str, curseforge_api_key: &str) -> Option<ModDownloadInfo> {
    println!("🔍 Procesando '{}' (ID: {:?}) v{} [{}]...", mod_name, mod_id.unwrap_or("N/A"), game_version, loader);
    
    // Helper para verificar y descargar
    let try_find_version = |hit: &ModrinthSearchHit| -> Option<ModDownloadInfo> {
        if let Some(modrinth_version) = fetch_modrinth_version(&hit.project_id, game_version, loader) {
            if let Some(file) = modrinth_version.first_file() {
                println!("✅ Encontrado en Modrinth: {} (Project: {})", file.filename, hit.title);
                // Convertimos del tipo `ModFile` a nuestro tipo unificado
                return Some(ModDownloadInfo {
                    filename: file.filename.clone(),
                    url: file.url.clone(),
                });
            }
        }
        None
    };

    // --- 1. Intento por ID (Si existe) ---
    if let Some(id) = mod_id {
        let hits = search_modrinth_project(id);
        for hit in &hits {
            if hit.slug == id {
                 if let Some(info) = try_find_version(hit) {
                     return Some(info);
                 }
            }
        }
    }

    // --- 2. Intento por Nombre (Fallback) ---
    let hits = search_modrinth_project(mod_name);
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
    if let Some(curseforge_id) = fetch_curseforge_project_id(mod_name, curseforge_api_key) {
        if let Some(curse_file) = fetch_curseforge_version_file(curseforge_id, game_version, loader, curseforge_api_key) {
            if let Some(download_url) = curse_file.download_url {
                println!("✅ Encontrado en CurseForge: {}", curse_file.file_name);
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
pub struct ModrinthSearchHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
}

fn build_modrinth_client() -> Client {
    Client::builder().user_agent("ModsUpdater/1.0 (github.com/FarlopaINC)").build().unwrap()
}

pub fn search_modrinth_project(query: &str) -> Vec<ModrinthSearchHit> {
    let client = build_modrinth_client();
    let search_url = "https://api.modrinth.com/v2/search";
    
    // Solicitamos 5 resultados para manejar ambigüedades
    let params = [
        ("query", query),
        ("limit", "5")
    ];

    match client.get(search_url).query(&params).send() {
        Ok(resp) => {
            if resp.status().is_success() {
                let results: ModrinthSearchResults = resp.json().unwrap_or(ModrinthSearchResults { hits: vec![] });
                return results.hits;
            } else {
                println!("❌ Error al buscar '{}' en la API (status {})", query, resp.status());
            }
        }
        Err(e) => {
            println!("❌ Error de conexión al buscar '{}': {}", query, e);
        }
    }
    return vec![];
}

pub fn fetch_modrinth_version(mod_id: &str, version: &str, loader: &str) -> Option<ModrinthVersion> {
    let client = build_modrinth_client();
    let api_url = format!("https://api.modrinth.com/v2/project/{}/version", mod_id);

    match client.get(&api_url).send() {
        Ok(resp) => {
            if resp.status().is_success() {
                let versions: Vec<ModrinthVersion> = resp.json().unwrap_or_default();
                let matching_version = versions.into_iter().find(|v| {
                    v.loaders.iter().any(|l| l.eq_ignore_ascii_case(loader)) && 
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

fn build_curse_client(api_key: &str) -> Client {
    let mut headers = header::HeaderMap::new();
    headers.insert("x-api-key", header::HeaderValue::from_str(api_key).unwrap());
    Client::builder()
        .default_headers(headers)
        .build()
        .unwrap()
}

pub fn fetch_curseforge_project_id(mod_name: &str, api_key: &str) -> Option<u32> {
    let client = build_curse_client(api_key);
    let search_url = "https://api.curseforge.com/v1/mods/search";
    
    // Parámetros seguros con codificación automática
    let params = [
        ("gameId", "432"),
        ("searchFilter", mod_name)
    ];

    match client.get(search_url).query(&params).send() {
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

pub fn fetch_curseforge_version_file(mod_id: u32, game_version: &str, loader: &str, api_key: &str) -> Option<CurseFile> {
    let client = build_curse_client(api_key);
    let api_url = format!("https://api.curseforge.com/v1/mods/{}/files", mod_id);

    // Map loader to CurseForge ID
    // 1 = Forge, 4 = Fabric, 5 = Quilt, 6 = NeoForge
    let loader_type = match loader.to_lowercase().as_str() {
        "forge" => "1",
        "fabric" => "4",
        "quilt" => "5",
        "neoforge" => "6",
        _ => "4", // Default to Fabric if unknown
    };

    let params = [
        ("gameVersion", game_version),
        ("modLoaderType", loader_type)
    ];

    match client.get(&api_url).query(&params).send() {
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
                