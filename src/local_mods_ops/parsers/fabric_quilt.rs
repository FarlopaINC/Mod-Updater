use serde::Deserialize;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;
use std::fs::File;
use crate::local_mods_ops::ModInfo;

// ── Deserialización de fabric.mod.json (Fabric / Quilt) ──────

#[derive(Debug, Deserialize)]
struct FabricModJson {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub depends: Option<std::collections::HashMap<String, serde_json::Value>>,
}

// ── Parser ───────────────────────────────────────────────────

pub fn try_parse(zip: &mut ZipArchive<File>, path: &Path) -> Option<ModInfo> {
    // Buscar fabric.mod.json (raíz del JAR)
    let mut json_str = String::new();
    for i in 0..zip.len() {
        if let Ok(mut file) = zip.by_index(i) {
            if Path::new(file.name()).file_name().and_then(|s| s.to_str()) == Some("fabric.mod.json") {
                if file.read_to_string(&mut json_str).is_ok() {
                    break;
                }
            }
        }
    }

    if json_str.is_empty() {
        return None;
    }

    let mod_json: FabricModJson = serde_json::from_str(&json_str).ok()?;

    let key = path.file_name().and_then(|s| s.to_str()).unwrap_or(&mod_json.name).to_string();

    let depends = mod_json.depends.map(|deps| {
        deps.into_iter().filter_map(|(k, v)| {
            match v {
                serde_json::Value::String(s) => Some((k, s)),
                serde_json::Value::Array(arr) => {
                    let s = arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                        .join(" || ");
                    if s.is_empty() { None } else { Some((k, s)) }
                },
                _ => None,
            }
        }).collect()
    });

    Some(ModInfo {
        key,
        name: mod_json.name,
        detected_project_id: Some(mod_json.id),
        confirmed_project_id: None,
        version_local: mod_json.version,
        version_remote: None,
        selected: true,
        file_size_bytes: None,
        file_mtime_secs: None,
        depends,
    })
}
