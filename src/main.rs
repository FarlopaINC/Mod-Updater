mod manage_mods;
mod utils;
mod ui;
mod download;
mod paths_vars;
use crate::ui::app::ModUpdaterApp;
use crate::manage_mods::{read_mods_in_folder, save_cache, load_cache};
use crate::paths_vars::PATHS;

fn main() {
    // Leemos los mods detectados en carpeta
    let mut detected = read_mods_in_folder(&PATHS.mods_folder.to_string_lossy().to_string());

    // Cargamos cache y fusionamos confirmed_project_id si existe
    let cache = load_cache();
    for (k, v) in cache {
        if let Some(entry) = detected.get_mut(&k) {
            if entry.confirmed_project_id.is_none() {
                entry.confirmed_project_id = v.confirmed_project_id.clone();
            }
        }
    }

    // Guardamos la cache actualizada
    save_cache(&detected);

    let options = eframe::NativeOptions::default();
    let _ = eframe::run_native(
        "Mods Updater",
        options,
        Box::new(|_cc| Ok(Box::new(ModUpdaterApp::new(detected)))),
    );
}
 