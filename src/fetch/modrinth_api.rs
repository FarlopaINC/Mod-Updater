use reqwest::blocking::Client;
use reqwest::header::HeaderMap;
use serde::Deserialize;
use serde_json::json;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use once_cell::sync::Lazy;

#[derive(Debug, Deserialize)]
pub struct ModrinthVersion {
    pub files: Vec<ModFile>,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub dependencies: Option<Vec<ModrinthDependency>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModrinthDependency {
    pub project_id: Option<String>,
    pub dependency_type: String,
}

impl ModrinthVersion {
    pub fn first_file(&self) -> Option<&ModFile> {
        self.files.first()
    }

    /// Devuelve las dependencias **requeridas** como Vec<UnifiedDependency>.
    /// Centraliza la extracción para que no se repita en fetch_from_api.
    pub fn required_deps(&self) -> Vec<crate::fetch::fetch_from_api::UnifiedDependency> {
        self.dependencies.as_ref()
            .map(|deps| {
                deps.iter()
                    .filter(|d| d.dependency_type == "required")
                    .filter_map(|d| d.project_id.clone())
                    .map(|id| crate::fetch::fetch_from_api::UnifiedDependency { mod_id: id })
                    .collect()
            })
            .unwrap_or_default()
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

// ── Rate Limiting ────────────────────────────────────────────

struct ModrinthRateLimit {
    remaining: u32,
    reset_at: Instant,
}

static RATE_LIMIT: Lazy<Mutex<ModrinthRateLimit>> = Lazy::new(|| {
    Mutex::new(ModrinthRateLimit {
        remaining: 300, // Asumir cubo lleno al inicio
        reset_at: Instant::now(),
    })
});

/// Si quedan pocas peticiones, duerme hasta que se resetee la ventana.
fn wait_for_ratelimit() {
    let state = RATE_LIMIT.lock().unwrap();
    if state.remaining < 5 {
        let now = Instant::now();
        if state.reset_at > now {
            let wait = state.reset_at - now;
            println!("⏳ Modrinth rate limit: esperando {:.1}s...", wait.as_secs_f32());
            drop(state); // Liberar el mutex antes de dormir
            std::thread::sleep(wait);
        }
    }
}

/// Actualiza el estado del rate limit con los headers de la respuesta.
fn update_ratelimit(headers: &HeaderMap) {
    let remaining = headers.get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u32>().ok());

    let reset_secs = headers.get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    if let (Some(rem), Some(reset)) = (remaining, reset_secs) {
        if let Ok(mut state) = RATE_LIMIT.lock() {
            state.remaining = rem;
            state.reset_at = Instant::now() + Duration::from_secs(reset);
        }
    }
}

// ── Client ───────────────────────────────────────────────────

static MODRINTH_CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder().user_agent("ModsUpdater/1.0 (github.com/FarlopaINC)").build().unwrap()
});

/// Consulta si Modrinth tiene capacidad de rate limit disponible (sin esperar).
pub fn has_capacity() -> bool {
    if let Ok(state) = RATE_LIMIT.lock() {
        state.remaining > 10 || Instant::now() >= state.reset_at
    } else {
        true // Si no podemos leer el mutex, asumir que sí hay capacidad
    }
}

// ── API Functions ────────────────────────────────────────────

pub fn search_modrinth_project(query: &str, loader: &Option<String>, version: &Option<String>, offset: u32, limit: u32) -> Vec<ModrinthSearchHit> {
    let client = &*MODRINTH_CLIENT;
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

    wait_for_ratelimit();

    match client.get(search_url).query(&params).send() {
        Ok(resp) => {
            update_ratelimit(resp.headers());
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

/// Fetches the human-readable title and slug of a Modrinth project by its ID or slug.
/// Used when we resolve a dependency by direct ID and have no search hit to pull the title from.
pub fn fetch_modrinth_project_info(mod_id: &str) -> Option<(String, String)> {
    #[derive(Deserialize)]
    struct ProjectInfo { title: String, slug: String }

    let client = &*MODRINTH_CLIENT;
    let api_url = format!("https://api.modrinth.com/v2/project/{}", mod_id);

    wait_for_ratelimit();

    match client.get(&api_url).send() {
        Ok(resp) => {
            update_ratelimit(resp.headers());
            if resp.status().is_success() {
                resp.json::<ProjectInfo>().ok().map(|p| (p.title, p.slug))
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

pub fn fetch_modrinth_version(mod_id: &str, version: &str, loader: &str) -> Option<ModrinthVersion> {
    let client = &*MODRINTH_CLIENT;
    let api_url = format!("https://api.modrinth.com/v2/project/{}/version", mod_id);

    // Prepare query parameters for filtering
    let loaders_json = json!([loader.to_lowercase()]).to_string();
    let versions_json = json!([version]).to_string();

    let params = [
        ("loaders", &loaders_json),
        ("game_versions", &versions_json),
    ];

    wait_for_ratelimit();

    match client.get(&api_url).query(&params).send() {
        Ok(resp) => {
            update_ratelimit(resp.headers());
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
