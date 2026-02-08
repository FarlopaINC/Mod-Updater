use reqwest::blocking::Client;
use reqwest::header;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    data: T,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CurseMod {
    pub id: u32,
    pub name: String,
    pub slug: String,
    pub summary: Option<String>,
    pub logo: Option<CurseLogo>,
    pub links: Option<CurseLinks>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CurseLogo {
    pub thumbnail_url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CurseLinks {
    pub website_url: String,
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

pub fn search_curseforge(query: &str, api_key: &str, loader: &Option<String>, version: &Option<String>, offset: u32, limit: u32) -> Vec<CurseMod> {
    let client = build_curse_client(api_key);
    let search_url = "https://api.curseforge.com/v1/mods/search";
    
    let limit_str = limit.to_string();
    let offset_str = offset.to_string();
    
    let mut params = vec![
        ("gameId", "432"),
        ("searchFilter", query),
        ("sortField", "2"), // Relevance
        ("sortOrder", "desc"),
        ("pageSize", &limit_str),
        ("index", &offset_str),
    ];
    
    if let Some(v) = version {
        if !v.trim().is_empty() {
            params.push(("gameVersion", v));
        }
    }
    
    // Map loader to CurseForge ID
    // 1 = Forge, 4 = Fabric, 5 = Quilt, 6 = NeoForge
    let loader_id_opt = if let Some(l) = loader {
         match l.to_lowercase().as_str() {
            "any" => Some("0"),
            "forge" => Some("1"),
            "cauldron" => Some("2"),
            "liteloader" => Some("3"),
            "fabric" => Some("4"),
            "quilt" => Some("5"),
            "neoforge" => Some("6"),
            _ => None, 
        }
    } else {
        None
    };

    if let Some(lid) = loader_id_opt {
        params.push(("modLoaderType", lid));
    }

    match client.get(search_url).query(&params).send() {
        Ok(resp) => {
            if resp.status().is_success() {
                let results: ApiResponse<Vec<CurseMod>> = resp.json().unwrap_or(ApiResponse { data: vec![] });
                return results.data;
            } else {
                println!("❌ Error al buscar '{}' en CurseAPI (status {})", query, resp.status());
            }
        }
        Err(e) => {
            println!("❌ Error de conexión al buscar en CurseForge: {}", e);
        }
    }
    return vec![];
}

pub fn fetch_curseforge_project_id(mod_name: &str, api_key: &str) -> Option<u32> {
    let results = search_curseforge(mod_name, api_key, &None, &None, 0, 10);
    results.first().map(|m| m.id)
}

pub fn fetch_curseforge_version_file(mod_id: u32, game_version: &str, loader: &str, api_key: &str) -> Option<CurseFile> {
    let client = build_curse_client(api_key);
    let api_url = format!("https://api.curseforge.com/v1/mods/{}/files", mod_id);

    // Map loader to CurseForge ID
    // 1 = Forge, 4 = Fabric, 5 = Quilt, 6 = NeoForge
    let loader_type = match loader.to_lowercase().as_str() {
        "any" => "0",
        "forge" => "1",
        "cauldron" => "2",
        "liteloader" => "3",
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
