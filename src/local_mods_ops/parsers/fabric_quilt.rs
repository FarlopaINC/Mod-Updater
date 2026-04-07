use serde::Deserialize;
use std::path::Path;
use zip::ZipArchive;
use std::fs::File;
use crate::local_mods_ops::ModInfo;
use super::read_zip_entry;

// ── Deserialización de fabric.mod.json (Fabric / Quilt) ──────

#[derive(Debug, Deserialize)]
struct FabricModJson {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub depends: Option<std::collections::HashMap<String, serde_json::Value>>,
    pub icon: Option<serde_json::Value>,
}

// ── Parser ───────────────────────────────────────────────────

pub fn try_parse(zip: &mut ZipArchive<File>, path: &Path) -> Option<ModInfo> {
    let json_str = read_zip_entry(zip, "fabric.mod.json")?;
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

    let mut has_local_icon = false;
    let mut possible_icons = vec!["pack.png".to_string(), "assets/icon.png".to_string()];
    if let Some(icon_val) = &mod_json.icon {
        if let Some(s) = icon_val.as_str() {
            possible_icons.insert(0, s.to_string());
        } else if let Some(obj) = icon_val.as_object() {
            for (_, path) in obj {
                if let Some(s) = path.as_str() {
                    possible_icons.insert(0, s.to_string());
                }
            }
        }
    }

    for path in possible_icons {
        if super::extract_and_save_icon(zip, &path, &mod_json.id) {
            has_local_icon = true;
            break;
        }
    }

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
        has_local_icon,
    })
}
