use reqwest::blocking::get;
use serde::Deserialize;

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

pub fn download_mod_file(file_url: &str, output_folder: &str, filename: &str) -> Result<(), std::io::Error> {
    println!("INFO: Descargando desde {}", file_url);
    let mut resp_file = get(file_url).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    let dest_path = format!("{}/{}", output_folder, filename);
    let mut out_file = std::fs::File::create(&dest_path)?;
    std::io::copy(&mut resp_file, &mut out_file)?;
    println!("✅ Descargado en {}", dest_path);
    Ok(())
}

                