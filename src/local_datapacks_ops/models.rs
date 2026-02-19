use serde::{Serialize, Deserialize};

/// Información de un datapack escaneado localmente.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DatapackInfo {
    pub key: String,                          // filename del .zip
    pub name: String,                         // mejor nombre encontrado
    pub detected_project_id: Option<String>,  // slug (id → namespace → filename)
    pub confirmed_project_id: Option<String>, // confirmado tras descarga exitosa
    pub pack_format: Option<u32>,             // del pack.mcmeta
    pub supported_formats: Option<(u32, u32)>, // (min, max) si existe
    pub mc_version: Option<String>,           // deducida de pack_format
    pub version_local: Option<String>,        // extraída del filename
    pub version_remote: Option<String>,
    pub selected: bool,
    #[serde(default)]
    pub file_size_bytes: Option<u64>,
    #[serde(default)]
    pub file_mtime_secs: Option<u64>,
}
