#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

use mods_updater::ui::app::ModUpdaterApp;


fn main() {
    // Inicializar DB (Redb)
    if !mods_updater::manage_mods::cache::init() {
        eprintln!("⚠️ Caché deshabilitada: la app funcionará sin caché (más lento).");
    }

    // Limpiar descargas parciales (.part) en hilo separado
    std::thread::spawn(|| {
        mods_updater::manage_mods::fs_ops::cleanup_partial_downloads();
    });

    let options = eframe::NativeOptions::default();
    let _ = eframe::run_native(
        "Mods Updater",
        options,
        Box::new(|cc| Ok(Box::new(ModUpdaterApp::new(cc)))),
    );
}
 