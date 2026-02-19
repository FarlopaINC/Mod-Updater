use crate::paths_vars::PATHS;
use super::models::DatapackInfo;
use indexmap::IndexMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;
use std::time::SystemTime;
use serde::Deserialize;

// ── Deserialización de pack.mcmeta ───────────────────────────

#[derive(Debug, Deserialize)]
struct PackMcMeta {
    pack: PackInfo,
}

#[derive(Debug, Deserialize)]
struct PackInfo {
    pack_format: u32,
    #[serde(default)]
    supported_formats: Option<SupportedFormats>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SupportedFormats {
    Range { min_inclusive: u32, max_inclusive: u32 },
    Array(Vec<u32>),
    Single(u32),
}

impl SupportedFormats {
    fn as_range(&self) -> (u32, u32) {
        return match self {
            SupportedFormats::Range { min_inclusive, max_inclusive } => (*min_inclusive, *max_inclusive),
            SupportedFormats::Array(arr) => {
                let min = arr.iter().copied().min().unwrap_or(0);
                let max = arr.iter().copied().max().unwrap_or(0);
                (min, max)
            }
            SupportedFormats::Single(v) => (*v, *v),
        };
    }
}

// ── Tabla pack_format → MC version (solo releases) ──────────

/// Devuelve la versión de MC correspondiente a un pack_format de datapack.
/// Solo cubre releases estables — los números intermedios son de snapshots.
pub fn pack_format_to_mc(format: u32) -> Option<&'static str> {
    return match format {
        4  => Some("1.13"),
        5  => Some("1.15"),
        6  => Some("1.16.5"),
        7  => Some("1.17.1"),
        8  => Some("1.18.1"),
        9  => Some("1.18.2"),
        10 => Some("1.19"),
        12 => Some("1.19.4"),
        15 => Some("1.20.1"),
        18 => Some("1.20.2"),
        26 => Some("1.20.4"),
        41 => Some("1.20.6"),
        48 => Some("1.21.1"),
        57 => Some("1.21.3"),
        61 => Some("1.21.4"),
        _  => None,
    };
}

// ── Utilidades ───────────────────────────────────────────────

fn get_file_mtime(metadata: &fs::Metadata) -> u64 {
    return metadata.modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
}

/// Intenta extraer una versión del filename.
/// Ej: "Terralith_1.21_v2.5.8.zip" → Some("2.5.8")
///     "epic_terrain-0.1.3-Beta.zip" → Some("0.1.3-Beta")
fn extract_version_from_filename(filename: &str) -> Option<String> {
    let name = filename.strip_suffix(".zip").unwrap_or(filename);
    
    // Buscar patrones comunes: vX.Y.Z, _vX.Y.Z, -X.Y.Z
    let patterns = [" v", "_v", "-v", " V", "_V", "-V"];
    for pat in &patterns {
        if let Some(pos) = name.find(pat) {
            let version = &name[pos + pat.len()..];
            if !version.is_empty() && version.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                return Some(version.to_string());
            }
        }
    }

    // Fallback: buscar último segmento que empiece con dígito tras separador
    for sep in ['-', '_'] {
        if let Some(pos) = name.rfind(sep) {
            let candidate = &name[pos + 1..];
            if candidate.chars().next().map_or(false, |c| c.is_ascii_digit()) 
               && candidate.contains('.') {
                return Some(candidate.to_string());
            }
        }
    }
    return None;
}

/// Intenta limpiar el filename para obtener un nombre legible.
/// Ej: "Terralith_1.21_v2.5.8.zip" → "Terralith"
fn clean_name_from_filename(filename: &str) -> String {
    let name = filename.strip_suffix(".zip").unwrap_or(filename);
    
    // Cortar en el primer separador seguido de un dígito (probablemente versión)
    for (i, c) in name.char_indices() {
        if (c == '_' || c == '-' || c == ' ') && i + 1 < name.len() {
            let next = name[i + 1..].chars().next().unwrap_or(' ');
            if next.is_ascii_digit() || next == 'v' || next == 'V' {
                let candidate = name[..i].trim();
                if !candidate.is_empty() {
                    return candidate.to_string();
                }
            }
        }
    }
    return name.to_string();
}

// ── Parser de datapack ───────────────────────────────────────

fn read_single_datapack(path: &Path) -> Result<DatapackInfo, String> {
    let file = File::open(path).map_err(|_| "No se pudo abrir archivo".to_string())?;
    let mut zip = ZipArchive::new(file).map_err(|_| "No es un ZIP válido".to_string())?;

    // 1. Leer pack.mcmeta
    let mut mcmeta_str = String::new();
    let mut found_mcmeta = false;
    for i in 0..zip.len() {
        if let Ok(mut entry) = zip.by_index(i) {
            let name = entry.name().replace('\\', "/");
            if name == "pack.mcmeta" {
                if entry.read_to_string(&mut mcmeta_str).is_ok() {
                    found_mcmeta = true;
                    break;
                }
            }
        }
    }

    if !found_mcmeta || mcmeta_str.is_empty() {
        return Err("No se encontró pack.mcmeta".to_string());
    }

    let pack_meta: PackMcMeta = serde_json::from_str(&mcmeta_str).map_err(|e| format!("Error parseando pack.mcmeta: {}", e))?;

    let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("unknown").to_string();

    // 2. Intentar obtener "id" del pack.mcmeta (campo custom, algunos lo tienen)
    let raw_json: serde_json::Value = serde_json::from_str(&mcmeta_str).unwrap_or_default();
    let id_from_meta = raw_json.get("pack")
        .and_then(|p| p.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // 3. Detectar namespaces en data/ (excluir "minecraft")
    let mut namespaces: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for i in 0..zip.len() {
        if let Ok(entry) = zip.by_index(i) {
            let name = entry.name().replace('\\', "/");
            if name.starts_with("data/") {
                let parts: Vec<&str> = name.split('/').collect();
                if parts.len() >= 2 && parts[1] != "minecraft" && !parts[1].is_empty() {
                    if seen.insert(parts[1].to_string()) {
                        namespaces.push(parts[1].to_string());
                    }
                }
            }
        }
    }

    // 4. Resolver detected_project_id
    let detected_project_id = if let Some(id) = id_from_meta {
        Some(id)
    } else if namespaces.len() == 1 {
        // Una sola namespace custom → muy probablemente es el slug
        Some(namespaces[0].clone())
    } else if !namespaces.is_empty() {
        // Varias namespaces → intentar match con filename limpio
        let clean = clean_name_from_filename(&filename).to_lowercase();
        namespaces.iter()
            .find(|ns| clean.contains(&ns.to_lowercase()) || ns.to_lowercase().contains(&clean))
            .cloned()
            .or_else(|| Some(namespaces[0].clone())) // fallback: primera namespace
    } else {
        None
    };

    // 5. Nombre y versión
    let name = clean_name_from_filename(&filename);
    let version_local = extract_version_from_filename(&filename);

    // 6. MC version desde pack_format
    let mc_version = pack_format_to_mc(pack_meta.pack.pack_format).map(|s| s.to_string());

    // 7. Supported formats
    let supported_formats = pack_meta.pack.supported_formats.map(|sf| sf.as_range());

    return Ok(DatapackInfo {
        key: filename,
        name,
        detected_project_id,
        confirmed_project_id: None,
        pack_format: Some(pack_meta.pack.pack_format),
        supported_formats,
        mc_version,
        version_local,
        version_remote: None,
        selected: true,
        file_size_bytes: None,
        file_mtime_secs: None,
    });
}

// ── Scanner de mundos y datapacks ────────────────────────────

/// Devuelve la lista de mundos en saves/
pub fn list_worlds() -> Vec<String> {
    let saves_folder = &PATHS.saves_folder;
    if !saves_folder.exists() {
        return vec![];
    }

    let mut worlds: Vec<String> = fs::read_dir(saves_folder)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    worlds.sort();
    return worlds;
}

/// Escanea los datapacks de un mundo específico.
pub fn read_datapacks_in_world(world_name: &str) -> IndexMap<String, DatapackInfo> {
    let datapacks_folder = PATHS.saves_folder.join(world_name).join("datapacks");
    let mut packs_map: IndexMap<String, DatapackInfo> = IndexMap::new();

    if !datapacks_folder.exists() {
        return packs_map;
    }

    if let Ok(entries) = fs::read_dir(&datapacks_folder) {
        let mut entries_vec: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries_vec.sort_by_key(|e| e.file_name());

        for entry in entries_vec {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("zip") {
                let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();

                let (file_size, file_mtime) = if let Ok(meta) = fs::metadata(&path) {
                    (meta.len(), get_file_mtime(&meta))
                } else {
                    (0, 0)
                };

                if let Ok(mut info) = read_single_datapack(&path) {
                    info.file_size_bytes = Some(file_size);
                    info.file_mtime_secs = Some(file_mtime);
                    packs_map.insert(filename, info);
                } else {
                    // Datapack sin metadata válido — registrar con info mínima
                    let name = clean_name_from_filename(&filename);
                    packs_map.insert(filename.clone(), DatapackInfo {
                        key: filename,
                        name,
                        file_size_bytes: Some(file_size),
                        file_mtime_secs: Some(file_mtime),
                        selected: true,
                        ..Default::default()
                    });
                }
            }
        }
    }
    return packs_map;
}
