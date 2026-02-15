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
