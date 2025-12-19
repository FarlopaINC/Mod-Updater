use eframe::{egui, egui::CentralPanel};
use eframe::egui::ScrollArea;
use eframe::egui::SidePanel;
use crate::manage_mods::{change_mods, copy_modpack_all, list_modpacks, get_minecraft_versions, read_mods_in_folder, read_active_marker, ModInfo};
use indexmap::IndexMap;
use std::ops::{Deref, DerefMut};
use crossbeam_channel::{unbounded, Sender, Receiver};
use crate::fetch::download::{spawn_workers, DownloadJob, DownloadEvent};
use crate::paths_vars::PATHS;

#[derive(Debug, Clone)]
pub enum ModStatus {
    Idle,
    Resolving,
    Downloading(f32), // progress 0.0 - 1.0
    Done,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct UiModInfo {
    pub inner: ModInfo,
    pub status: ModStatus,
    pub progress: f32,
}

impl From<ModInfo> for UiModInfo {
    fn from(m: ModInfo) -> Self {
        UiModInfo { inner: m, status: ModStatus::Idle, progress: 0.0 }
    }
}

impl Deref for UiModInfo {
    type Target = ModInfo;
    fn deref(&self) -> &ModInfo { &self.inner }
}

impl DerefMut for UiModInfo {
    fn deref_mut(&mut self) -> &mut ModInfo { &mut self.inner }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeMode {
    Symlink, // intenta enlace/junction y hace fallback a rename/copia
    Copy,    // fuerza la copia (preserva el modpack original)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeletionConfirmation {
    None,
    Modpack(String),
    SelectedMods,
}

pub struct ModUpdaterApp {
    pub mods: IndexMap<String, UiModInfo>,
    pub mc_versions: Vec<String>,
    pub selected_mc_version: String,
    tx_jobs: Sender<DownloadJob>,
    rx_events: Receiver<DownloadEvent>,
    deletion_confirmation: DeletionConfirmation,
    pub change_mode: ChangeMode,
    pub status_msg: String,
}

impl ModUpdaterApp {
    pub fn new(mods: IndexMap<String, ModInfo>) -> Self {
        let mc_versions = get_minecraft_versions(&PATHS.versions_folder
            .join("version_manifest_V2.json")
            .to_string_lossy()
            .to_string()
        );
        let selected_mc_version = mc_versions.get(0).cloned().unwrap_or_else(|| "1.20.2".to_string());
        // Merge with cache (if present) so we keep confirmed IDs / remote version info
        let cache = crate::manage_mods::load_cache();
        let mut ui_mods: IndexMap<String, UiModInfo> = IndexMap::new();
        for (k, v) in mods.into_iter() {
            if let Some(cached) = cache.get(&k) {
                let mut merged = cached.clone();
                // preserve current selection state from detected folder
                merged.selected = v.selected;
                ui_mods.insert(k.clone(), UiModInfo::from(merged));
            } else {
                ui_mods.insert(k.clone(), UiModInfo::from(v));
            }
        }

        // create channels
        let (tx_jobs, rx_jobs) = unbounded();
        let (tx_events, rx_events) = unbounded();

        // Compute worker count based on system and number of mods
        let mods_count = ui_mods.len();
        let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        // Downloads are IO-bound: allow several threads per CPU but cap to avoid excess
        let mut workers = std::cmp::min(mods_count.max(1), cpus.saturating_mul(4));
        if workers == 0 { workers = 1; }
        if workers > 32 { workers = 32; }

        spawn_workers(workers, rx_jobs, tx_events);

        // persist any new entries in cache (optional)
        let mut map_for_save: IndexMap<String, ModInfo> = IndexMap::new();
        for (k, v) in &ui_mods { map_for_save.insert(k.clone(), v.inner.clone()); }
        crate::manage_mods::save_cache(&map_for_save);

        return Self { mods: ui_mods, mc_versions, selected_mc_version, tx_jobs, rx_events, deletion_confirmation: DeletionConfirmation::None, change_mode: ChangeMode::Symlink, status_msg: String::new() };
    }
}

impl eframe::App for ModUpdaterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Panel lateral derecho: Selector de modpacks
        let is_confirming_deletion = self.deletion_confirmation != DeletionConfirmation::None;
        SidePanel::right("selector_modpacks")
            .resizable(false)
            .default_width(180.0)
            .show(ctx, |ui| {
                ui.heading("Modpacks");
                ui.add_space(5.0);

                // Selector de modo de cambio (tooltip explica las opciones)
                ui.horizontal(|ui| {
                    ui.label("MODO:").on_hover_text("Enlace: intenta crear un acceso directo del modpack que sustituya la carpeta de \"mods\" (rapido). Si no funciona se usa el modo \"copiar\".
                        \nCopiar: copia y pega los modpacks completos en la carpeta \"mods\" (mas lento pero funciona siempre).");

                    egui::ComboBox::from_id_salt("change-mode")
                        .selected_text(match self.change_mode {
                            ChangeMode::Symlink => "Enlace",
                            ChangeMode::Copy => "Copiar",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.change_mode, ChangeMode::Symlink, "Enlace");
                            ui.selectable_value(&mut self.change_mode, ChangeMode::Copy, "Copiar");
                        });
                });

                ui.add_space(6.0);

                let modpacks = list_modpacks();

                if modpacks.is_empty() {
                    ui.label("No hay modpacks disponibles");
                } else {
                    ScrollArea::vertical().show(ui, |ui| {
                        for mp in modpacks {
                            ui.horizontal(|ui| {
                                let seleccionado = match PATHS.mods_folder.read_link() {
                                    Ok(link) => link.ends_with(&mp),
                                    Err(_) => match read_active_marker() {
                                        Some(active) => active == mp,
                                        None => false,
                                    },
                                };
                                if seleccionado {
                                    ui.label(format!("{} ✅", mp));
                                } else {
                                    ui.label(&mp);
                                    if ui.button("Cambiar").clicked() {
                                        self.status_msg = match self.change_mode {
                                            ChangeMode::Symlink => change_mods(&mp).unwrap(),
                                            ChangeMode::Copy => {
                                                let target = &PATHS.mods_folder;
                                                let source = &PATHS.modpacks_folder.join(&mp);
                                                match copy_modpack_all(source, target) {
                                                    Ok(()) => format!("Mods cambiados a '{}'.", &mp),
                                                    Err(e) => format!("Error al copiar modpack: {}", e),
                                                }
                                            }
                                        }
                                    }
                                }

                                // Button to delete this modpack
                                ui.add_space(4.0);
                                if ui.button("🗑").clicked() {
                                    self.deletion_confirmation = DeletionConfirmation::Modpack(mp.clone());
                                }
                            });
                            ui.separator();
                        }
                    });
                }
                // Mostrar mensaje de estado
                if !self.status_msg.is_empty() {
                    ui.separator();
                    ui.label(&self.status_msg);
                }     
            });
        ctx.request_repaint(); // Para que la UI se actualice mientras se descarga

        CentralPanel::default().show(ctx, |ui| {
            ui.heading("Mods instalados");
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label("Versión:").on_hover_text("Versión a la que se va a actualizar");
                egui::ComboBox::from_id_salt("mc-version-box")
                    .selected_text(&self.selected_mc_version)
                    .show_ui(ui, |ui| {
                        for v in &self.mc_versions {
                            ui.selectable_value(&mut self.selected_mc_version, v.clone(), v);
                        }
                        ui.separator();
                        ui.label("Personalizada:");
                        ui.text_edit_singleline(&mut self.selected_mc_version);
                    });
        
                
                if ui.button("🔁")
                    .on_hover_text("Actualiza la lista de mods leyendo la carpeta actual.")
                    .clicked() {
                        let detected = read_mods_in_folder(&PATHS.mods_folder.to_string_lossy().to_string());
                        let ui_mods: IndexMap<String, UiModInfo> = detected.into_iter().map(|(k, v)| (k, UiModInfo::from(v))).collect();
                        self.mods = ui_mods; 
                        self.status_msg = "Lista de mods actualizada".to_string();
                }

                
                if ui.button("⬇")
                    .on_hover_text("Descarga los mods seleccionados en la versión escogida.")
                    .clicked() {
                        // Encolar trabajos para los mods seleccionados
                        for (k, m) in self.mods.clone().into_iter() {
                            if m.selected {
                                crate::manage_mods::prepare_output_folder(&self.selected_mc_version);
                                let output_folder_path = PATHS.modpacks_folder.join(format!(r"mods{}", self.selected_mc_version));
                                let job = DownloadJob {
                                    key: k.clone(),
                                    modinfo: m.inner.clone(),
                                    output_folder: output_folder_path.to_string_lossy().to_string(),
                                    selected_version: self.selected_mc_version.clone()
                                };
                                let _ = self.tx_jobs.send(job);
                            }
                        }
                }
                // Button to delete selected mods
                if ui.button("🗑")
                    .on_hover_text("Elimina los archivos .jar de los mods seleccionados")
                    .clicked() {
                    self.deletion_confirmation = DeletionConfirmation::SelectedMods;
                }

                let all_selected = self.mods.values().all(|m| m.selected);
                let select_label = if all_selected { "✅" } else { "⬜" };
                if ui.button(select_label)
                    .on_hover_text("Alterna la selección de todos los mods")
                    .clicked() {
                    if all_selected {
                        for m in self.mods.values_mut() { m.selected = false; }                       
                    } else {
                        for m in self.mods.values_mut() { m.selected = true; }
                    }
                }
            });

            ui.add_space(8.0);
            ScrollArea::vertical().show(ui, |ui| {
                let keys: Vec<String> = self.mods.keys().cloned().collect();
                for key in keys {
                    if let Some(m) = self.mods.get_mut(&key) {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut m.selected, "");
                            ui.label(&m.name);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add_space(8.0); // Margen derecho para el scrollbar
                                match &m.status {
                                    ModStatus::Idle => {},
                                    ModStatus::Resolving => { ui.label("⌛ Resolviendo..."); },
                                    ModStatus::Downloading(progress) => { ui.label(format!("📥 {}%", (progress * 100.0) as i32)); },
                                    ModStatus::Done => { ui.label("✅ Completado"); },
                                    ModStatus::Error(msg) => { ui.label(format!("❌ Error: {}", msg)); },
                                }
                            });
                        });
                    }
                }
            });
            ui.separator();

            // Procesar eventos entrantes desde los workers
            for ev in self.rx_events.try_iter() {
                match ev {
                    DownloadEvent::Resolving { key } => {
                        if let Some(m) = self.mods.get_mut(&key) { m.status = ModStatus::Resolving; }
                    }
                    DownloadEvent::Resolved { key } => {
                        if let Some(m) = self.mods.get_mut(&key) { m.status = ModStatus::Idle; }
                    }
                    DownloadEvent::Started { key } => {
                        if let Some(m) = self.mods.get_mut(&key) { m.status = ModStatus::Downloading(0.0); }
                    }
                    DownloadEvent::ResolvedInfo { key, confirmed_project_id, version_remote } => {
                        if let Some(m) = self.mods.get_mut(&key) {
                            m.inner.confirmed_project_id = confirmed_project_id.clone();
                            m.inner.version_remote = version_remote.clone();

                            // Save updated cache to disk (UI thread owns cache writes)
                            let mut map_for_save: IndexMap<String, crate::manage_mods::ModInfo> = IndexMap::new();
                            for (k, v) in &self.mods { map_for_save.insert(k.clone(), v.inner.clone()); }
                            crate::manage_mods::save_cache(&map_for_save);
                        }
                    }
                    DownloadEvent::Done { key } => {
                        if let Some(m) = self.mods.get_mut(&key) { m.status = ModStatus::Done; m.progress = 1.0; }
                    }
                    DownloadEvent::Error { key, msg } => {
                        if let Some(m) = self.mods.get_mut(&key) { m.status = ModStatus::Error(msg); }
                    }
                }
            }
        });

        // --- Ventana de confirmación de borrado ---
        if is_confirming_deletion {
            egui::Window::new("Confirmar borrado")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    let message = match &self.deletion_confirmation {
                        DeletionConfirmation::Modpack(name) => format!("¿Seguro que quieres borrar el modpack '{}'?", name),
                        DeletionConfirmation::SelectedMods => "¿Seguro que quieres borrar los mods seleccionados?".to_string(),
                        DeletionConfirmation::None => "".to_string(),
                    };
                    ui.label(message);
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Cancelar").clicked() {
                            self.deletion_confirmation = DeletionConfirmation::None;
                        }
                        ui.add_space(10.0);
                        if ui.button("Aceptar").clicked() {
                            match self.deletion_confirmation.clone() {
                                DeletionConfirmation::Modpack(mp_name) => {
                                    let target = PATHS.modpacks_folder.join(&mp_name);
                                    match std::fs::remove_dir_all(&target) {
                                        Ok(_) => { self.status_msg = format!("Modpack '{}' borrado.", mp_name); }
                                        Err(e) => { self.status_msg = format!("Error borrando modpack '{}': {}", mp_name, e); }
                                    }
                                }
                                DeletionConfirmation::SelectedMods => {
                                    let mut removed_keys: Vec<String> = Vec::new();
                                    // Usamos retain_mut para iterar y modificar en el mismo lugar
                                    self.mods.retain(|key, m| {
                                        if m.selected {
                                            // Usamos la ruta canónica para asegurar que borramos el archivo real
                                            // incluso si `mods` es un enlace simbólico.
                                            let path = match PATHS.mods_folder.canonicalize() {
                                                Ok(p) => p.join(key),
                                                Err(_) => PATHS.mods_folder.join(key), // Fallback a la ruta normal
                                            };

                                            match std::fs::remove_file(&path) {
                                                Ok(_) => {
                                                    removed_keys.push(key.clone());
                                                    false // Eliminar de self.mods
                                                },
                                                Err(e) => {
                                                    self.status_msg = format!("Error borrando {}: {}", key, e);
                                                    true // Mantener en self.mods si hay error
                                                }
                                            }
                                        } else {
                                            true // Mantener si no está seleccionado
                                        }
                                    });
                                    if !removed_keys.is_empty() {
                                        self.status_msg = format!("Borrados {} mods seleccionados.", removed_keys.len());
                                    }
                                }
                                _ => {}
                            }
                            self.deletion_confirmation = DeletionConfirmation::None;
                        }
                    });
                });
        }
    }
}
