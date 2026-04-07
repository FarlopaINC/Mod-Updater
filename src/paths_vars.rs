use std::path::PathBuf;
use once_cell::sync::Lazy;

pub static PATHS: Lazy<Paths> = Lazy::new(|| {
    let base = get_default_game_folder().unwrap_or_else(|| {
        panic!("No se pudo detectar la carpeta de Minecraft");
    });

    Paths::new(base)
});

#[allow(dead_code)]
pub struct Paths {
    pub base_game_folder: PathBuf,
    pub mods_folder: PathBuf,
    pub versions_folder: PathBuf,
    pub modpacks_folder: PathBuf,
    pub saves_folder: PathBuf,
    pub icons_folder: PathBuf,
}

impl Paths {
    pub fn new(base_game_path: PathBuf) -> Self {
        let mut icons = dirs::cache_dir().unwrap_or_else(|| base_game_path.clone());
        icons.push("mods_updater");
        icons.push("icons");
        let _ = std::fs::create_dir_all(&icons);

        Self {
            mods_folder: base_game_path.join("mods"),
            versions_folder: base_game_path.join("versions"),
            modpacks_folder: base_game_path.join("modpacks"),
            saves_folder: base_game_path.join("saves"),
            base_game_folder: base_game_path,
            icons_folder: icons,
        }
    }
}

pub fn get_default_game_folder() -> Option<PathBuf> {
    // Windows: AppData/Roaming/.minecraft
    if let Some(mut dir) = dirs::data_dir() {
        // En Windows devuelve AppData\Roaming
        dir.push(".minecraft");
        if dir.exists() {
            return Some(dir);
        }
    }

    // Linux: ~/.minecraft
    if let Some(mut dir) = dirs::home_dir() {
        dir.push(".minecraft");
        if dir.exists() {
            return Some(dir);
        }
    }

    // macOS: ~/Library/Application Support/minecraft
    if let Some(mut dir) = dirs::home_dir() {
        dir.push("Library/Application Support/minecraft");
        if dir.exists() {
            return Some(dir);
        }
    }
    return None;
}