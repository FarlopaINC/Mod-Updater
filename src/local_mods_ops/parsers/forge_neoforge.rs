use serde::Deserialize;
use std::path::Path;
use zip::ZipArchive;
use std::fs::File;
use crate::local_mods_ops::ModInfo;
use super::read_zip_entry;

// ── Deserialización de META-INF/mods.toml (Forge / NeoForge) ─

#[derive(Debug, Deserialize)]
struct ForgeModToml {
    #[serde(default)]
    mods: Vec<ForgeModEntry>,
    #[serde(default)]
    dependencies: Option<toml::Value>, // Estructura flexible: [[dependencies.modId]]
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ForgeModEntry {
    mod_id: String,
    version: Option<String>,
    display_name: Option<String>,
}

// ── Parser ───────────────────────────────────────────────────

pub fn try_parse(zip: &mut ZipArchive<File>, path: &Path) -> Option<ModInfo> {
    let toml_str = read_zip_entry(zip, "META-INF/mods.toml")?;
    let mod_toml: ForgeModToml = toml::from_str(&toml_str).ok()?;
    let entry = mod_toml.mods.first()?;

    let key = path.file_name().and_then(|s| s.to_str())
        .unwrap_or(entry.display_name.as_deref().unwrap_or(&entry.mod_id))
        .to_string();

    let name = entry.display_name.clone().unwrap_or_else(|| entry.mod_id.clone());

    // Extraer dependencias de [[dependencies.modId]]
    let depends = mod_toml.dependencies.and_then(|deps| {
        let table = deps.as_table()?;
        // Las claves son los modId del mod principal, los valores son arrays de deps
        let mut all_deps = std::collections::HashMap::new();
        for (_mod_id, dep_array) in table {
            if let Some(arr) = dep_array.as_array() {
                for dep in arr {
                    if let Some(dep_table) = dep.as_table() {
                        let dep_mod_id = dep_table.get("modId")?.as_str()?.to_string();
                        let version_range = dep_table.get("versionRange")
                            .and_then(|v| v.as_str())
                            .unwrap_or("*")
                            .to_string();
                        all_deps.insert(dep_mod_id, version_range);
                    }
                }
            }
        }
        if all_deps.is_empty() { None } else { Some(all_deps) }
    });

    // Limpiar versión (Forge usa ${file.jarVersion} como placeholder)
    let version = entry.version.clone().filter(|v| !v.contains("${"));

    Some(ModInfo {
        key,
        name,
        detected_project_id: Some(entry.mod_id.clone()),
        confirmed_project_id: None,
        version_local: version,
        version_remote: None,
        selected: true,
        file_size_bytes: None,
        file_mtime_secs: None,
        depends,
    })
}
