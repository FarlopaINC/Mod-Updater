use serde::{Serialize, Deserialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FixAction {
    Rename { target_name: String },
    EditMetadata { field: String, new_value: String },
    Delete,
    Ignore,
    ManualReview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownIssue {
    pub mod_id_or_name: String, // Identificador del mod problemático
    pub issue_description: String,
    pub fix: FixAction,
    pub auto_apply: bool, // Si es true, se aplica sin preguntar
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TroubleshootMemory {
    pub issues: Vec<KnownIssue>,
}

impl TroubleshootMemory {
    pub fn new() -> Self {
        Self { issues: Vec::new() }
    }

    pub fn check(&self, mod_name: &str) -> Option<&KnownIssue> {
        self.issues.iter().find(|issue| {
             issue.mod_id_or_name.eq_ignore_ascii_case(mod_name)
        })
    }

    pub fn add_issue(&mut self, issue: KnownIssue) {
        // Evitar duplicados simples
        if self.check(&issue.mod_id_or_name).is_none() {
            self.issues.push(issue);
        }
    }
}

// Persistencia
fn memory_path() -> Option<PathBuf> {
    use crate::paths_vars::PATHS;
    let dir = &PATHS.modpacks_folder;
    if !dir.exists() {
        let _ = fs::create_dir_all(dir);
    }
    Some(dir.join("problematic_mods.json"))
}

pub fn load_memory() -> TroubleshootMemory {
    if let Some(path) = memory_path() {
        if path.exists() {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(mem) = serde_json::from_str::<TroubleshootMemory>(&data) {
                    return mem;
                }
            }
        }
    }
    TroubleshootMemory::new()
}

pub fn save_memory(mem: &TroubleshootMemory) {
    if let Some(path) = memory_path() {
        if let Ok(data) = serde_json::to_string_pretty(mem) {
            let tmp = path.with_extension("json.tmp");
            if fs::write(&tmp, &data).is_ok() {
                let _ = fs::rename(&tmp, &path);
            }
        }
    }
}
