use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
pub struct ModrinthVersion {
    pub files: Vec<ModFile>,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
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

#[derive(Debug, Deserialize, Clone)]
pub struct ModrinthSearchHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub author: String,
}

fn build_modrinth_client() -> Client {
    Client::builder().user_agent("ModsUpdater/1.0 (github.com/FarlopaINC)").build().unwrap()
}

pub fn search_modrinth_project(query: &str, loader: &Option<String>, version: &Option<String>, offset: u32, limit: u32) -> Vec<ModrinthSearchHit> {
    let client = build_modrinth_client();
    let search_url = "https://api.modrinth.com/v2/search";
    
    let mut facets = Vec::new();
    if let Some(v) = version {
        if !v.trim().is_empty() {
             facets.push(format!("[\"versions:{}\"]", v));
        }
    }
    if let Some(l) = loader {
        if !l.trim().is_empty() {
            facets.push(format!("[\"categories:{}\"]", l.to_lowercase()));
        }
    }
    
    let facets_str = if facets.is_empty() {
        String::new()
    } else {
        format!("[{}]", facets.join(","))
    };

    let limit_str = limit.to_string();
    let offset_str = offset.to_string();
    
    let mut params = vec![
        ("query", query),
        ("limit", &limit_str),
        ("offset", &offset_str),
    ];
    
    if !facets.is_empty() {
        params.push(("facets", &facets_str));
    }

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

    // Prepare query parameters for filtering
    let loaders_json = json!([loader.to_lowercase()]).to_string();
    let versions_json = json!([version]).to_string();

    let params = [
        ("loaders", &loaders_json),
        ("game_versions", &versions_json),
    ];

    match client.get(&api_url).query(&params).send() {
        Ok(resp) => {
            if resp.status().is_success() {
                let versions: Vec<ModrinthVersion> = resp.json().unwrap_or_default();
                return versions.into_iter().next();
            } else {
                println!("❌ Error en API para {}: status {}", mod_id, resp.status());
                return None;
            }
        }
        Err(e) => {
            println!("❌ Error consultando API de Modrinth: {}", e);
            return None;
        }
    }
}
