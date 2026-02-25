use serde::{Serialize, Deserialize};

// Información básica de un mod (Utilizada en la UI y lógica general)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModInfo {
    pub key: String,
    pub name: String,
    pub detected_project_id: Option<String>,
    pub confirmed_project_id: Option<String>,
    pub version_local: Option<String>,
    pub version_remote: Option<String>,
    pub selected: bool,
    #[serde(default)]
    pub file_size_bytes: Option<u64>,
    #[serde(default)]
    pub file_mtime_secs: Option<u64>,
    #[serde(default)]
    pub depends: Option<std::collections::HashMap<String, String>>,
}

impl ModInfo {
    /// Mod añadido manualmente desde búsqueda (botón ADD en perfil).
    /// `key` y `name` son el título del proyecto; `project_id` es el ID de Modrinth/CurseForge.
    pub fn from_search(name: String, project_id: Option<String>) -> Self {
        Self {
            key: name.clone(),
            name,
            detected_project_id: project_id.clone(),
            confirmed_project_id: project_id,
            version_local: Some("Universal".to_string()),
            selected: true,
            ..Default::default()
        }
    }

    /// Dependencia resuelta automáticamente por el BFS.
    /// `filename` es la clave del IndexMap; `slug` se almacena como `detected_project_id`
    /// para que el dedup pueda compararlo contra mods escaneados del disco.
    pub fn from_dep(filename: String, name: String, project_id: String, slug: String) -> Self {
        Self {
            key: filename.clone(),
            name,
            detected_project_id: Some(slug),
            confirmed_project_id: Some(project_id),
            version_local: Some("Universal".to_string()),
            selected: true,
            ..Default::default()
        }
    }
}


/// Estructuras para parsear el manifest de versiones de Minecraft
#[derive(Deserialize, Debug)]
pub struct VersionInfo {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: String,
}

#[derive(Deserialize, Debug)]
pub struct VersionManifest {
    pub versions: Vec<VersionInfo>,
}

// --- Cache Models (Internal) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedProject {
    pub name: String,
    pub detected_project_id: Option<String>,
    pub confirmed_project_id: Option<String>,
    pub version_remote: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFile {
    pub file_size_bytes: Option<u64>,
    pub file_mtime_secs: Option<u64>,
    pub version_local: Option<String>,
    pub depends: Option<std::collections::HashMap<String, String>>,
    pub project_ref: String, // Reference to project_id (usually detected_project_id or fallback)
}
