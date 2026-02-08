use indexmap::IndexMap;
use serde::{Serialize, Deserialize};
use super::models::ModInfo;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub created_at: u64,
    pub description: Option<String>,
    pub mods: IndexMap<String, ModInfo>,
}

impl Profile {
    pub fn new(name: String, description: Option<String>) -> Self {
        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();

        Self {
            name,
            created_at: since_the_epoch,
            description,
            mods: IndexMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfilesDatabase {
    pub profiles: IndexMap<String, Profile>,
}

impl ProfilesDatabase {
    pub fn new() -> Self {
        Self { profiles: IndexMap::new() }
    }

    pub fn add_profile(&mut self, profile: Profile) {
        self.profiles.insert(profile.name.clone(), profile);
    }

    pub fn get_profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }

    pub fn get_profile_mut(&mut self, name: &str) -> Option<&mut Profile> {
        self.profiles.get_mut(name)
    }

    pub fn delete_profile(&mut self, name: &str) {
        self.profiles.shift_remove(name);
    }
}

// Persistencia
fn profiles_path() -> Option<PathBuf> {
    use crate::paths_vars::PATHS;
    let dir = &PATHS.modpacks_folder;
    if !dir.exists() {
        let _ = fs::create_dir_all(dir);
    }
    Some(dir.join("profiles.json"))
}

pub fn load_profiles() -> ProfilesDatabase {
    if let Some(path) = profiles_path() {
        if path.exists() {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(db) = serde_json::from_str::<ProfilesDatabase>(&data) {
                    return db;
                }
            }
        }
    }
    ProfilesDatabase::new()
}

pub fn save_profiles(db: &ProfilesDatabase) {
    if let Some(path) = profiles_path() {
        if let Ok(data) = serde_json::to_string_pretty(db) {
            let _ = fs::write(path, data);
        }
    }
}
