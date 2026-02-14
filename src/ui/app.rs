use eframe::{egui, egui::CentralPanel};
use std::collections::{HashSet, HashMap};
use eframe::egui::{ScrollArea, SidePanel, TopBottomPanel};
use std::ops::{Deref, DerefMut};
use crossbeam_channel::{unbounded, Sender, Receiver};
use std::thread;
use indexmap::IndexMap;

use crate::manage_mods::{
    change_mods, list_modpacks, get_minecraft_versions, read_active_marker,
    spawn_read_workers, ReadJob, ReadEvent,
    ModInfo, ProfilesDatabase, TroubleshootMemory, load_profiles, save_profiles, load_memory, Profile,
};
use crate::fetch::async_download::{spawn_workers, DownloadJob, DownloadEvent};
use crate::fetch::single_mod_search::{UnifiedSearchResult, search_unified, SearchRequest};
use crate::paths_vars::PATHS;
use super::utils::{format_dep_name, format_version_range};  

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum SearchSource {
    Explorer,
    Profile(String),
}

pub struct SearchState {
    pub query: String,
    pub loader: String,
    pub version: String,
    pub results: Vec<UnifiedSearchResult>,
    pub is_searching: bool,
    pub open: bool,
    pub source: SearchSource,
    pub page: u32,
    pub limit: u32,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            query: String::new(),
            loader: "Fabric".to_string(), // Default, will be overwritten by app selection
            version: "1.20.1".to_string(),
            results: Vec::new(),
            is_searching: false,
            open: false,
            source: SearchSource::Explorer,
            page: 0,
            limit: 10,
        }
    }
}

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



#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeletionConfirmation {
    None,
    Modpack(String),
    SelectedMods,
    Profile(String), // Confirm deletion of a profile
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadSource {
    Explorer,
    Profile(String),
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum AppTab {
    Explorer,
    Profiles,
}

pub struct ModUpdaterApp {
    // --- Shared State ---
    pub mc_versions: Vec<String>,
    pub selected_mc_version: String,
    pub status_msg: String,
    pub current_tab: AppTab,

    // --- Explorer State ---
    pub mods: IndexMap<String, UiModInfo>,
    tx_jobs: Sender<DownloadJob>,
    rx_events: Receiver<DownloadEvent>,
    
    // --- Async Read State ---
    tx_read_jobs: Sender<ReadJob>,
    rx_read_events: Receiver<ReadEvent>,

    deletion_confirmation: DeletionConfirmation,

    // --- Profiles State ---
    pub profiles_db: ProfilesDatabase,
    pub memory: TroubleshootMemory,
    pub selected_profile_name: Option<String>,
    pub profile_mods_pending_deletion: HashSet<String>,


    // --- UI Selection State ---
    pub loaders: Vec<String>,
    pub selected_loader: String,
    pub selected_modpack_ui: Option<String>,
    
    // --- Download Dialog State ---
    pub download_confirmation_name: Option<String>,
    pub download_source: DownloadSource,
    
    // --- Create Profile Dialog State ---
    pub create_profile_modal_name: Option<String>,
    
    // --- Modpacks State ---
    pub active_modpack: Option<String>,
    pub cached_modpacks: Vec<String>,
    
    // --- Global Download State ---
    pub active_downloads: HashMap<String, ModStatus>,

    // --- Search State ---
    pub search_state: SearchState,
    tx_search: Sender<(SearchRequest, SearchSource)>, // Request, Source
    rx_search: Receiver<(Vec<UnifiedSearchResult>, SearchSource, u32)>, // Results, Source, Offset
}

impl ModUpdaterApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Detect active modpack first to use in logic
        let active_modpack = crate::manage_mods::fs_ops::read_active_marker();

        let mc_versions = get_minecraft_versions(&PATHS.versions_folder
            .join("version_manifest_V2.json")
            .to_string_lossy()
            .to_string()
        );
        let selected_mc_version = mc_versions.get(0).cloned().unwrap_or_else(|| "1.20.2".to_string());
        
        // Initial empty state
        let mut ui_mods: IndexMap<String, UiModInfo> = IndexMap::new();
        
        // create channels
        let (tx_jobs, rx_jobs) = unbounded();
        let (tx_events, rx_events) = unbounded();

        // Create async read channels EARLY so we can use them
        let (tx_read_jobs, rx_read_jobs) = unbounded();
        let (tx_read_events, rx_read_events) = unbounded();

        // If we have an active modpack, scan it with cache check
        if let Some(ref pack_name) = active_modpack {
            let pack_folder = PATHS.modpacks_folder.join(pack_name);
            if let Ok(entries) = std::fs::read_dir(&pack_folder) {
                let mut entries_vec: Vec<_> = entries.filter_map(|e| e.ok()).collect();
                entries_vec.sort_by_key(|e| e.file_name());

                for entry in entries_vec {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("jar") {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        let (file_size, file_mtime) = if let Ok(meta) = std::fs::metadata(&path) {
                            (meta.len(), crate::manage_mods::scanner::get_file_mtime(&meta))
                        } else { (0, 0) };

                        let mut loaded = false;
                        if let Some(cached) = crate::manage_mods::cache::get_mod(&filename) {
                            if cached.file_size_bytes == Some(file_size) && cached.file_mtime_secs == Some(file_mtime) {
                                ui_mods.insert(cached.key.clone(), UiModInfo::from(cached));
                                loaded = true;
                            }
                        }
                        if !loaded {
                            let _ = tx_read_jobs.send(ReadJob { file_path: path });
                        }
                    }
                }
            }
        }

        // Compute worker count
        let mods_count = ui_mods.len();
        let workers = crate::ui::utils::calculate_worker_count(20.max(mods_count));

        spawn_workers(workers, rx_jobs, tx_events);

        // Spawn read workers
        spawn_read_workers(workers, rx_read_jobs, tx_read_events.clone());

        // Search Channel
        let (tx_search, rx_search_req) = unbounded::<(SearchRequest, SearchSource)>();
        let (tx_search_res, rx_search) = unbounded();
        
        // Spawn Search Worker
        {
            let tx_res = tx_search_res.clone();
            thread::spawn(move || {
                while let Ok((req, source)) = rx_search_req.recv() {
                    let offset = req.offset;
                    let results = search_unified(&req);
                    let _ = tx_res.send((results, source, offset));
                }
            });
        }

        // Load Profiles and Memory
        let profiles_db = load_profiles();
        let memory = load_memory();

        // --- Background Cache Cleanup ---
        thread::spawn(|| {
            // Wait a bit to let the app load critical stuff first
            thread::sleep(std::time::Duration::from_secs(5));
            crate::manage_mods::cache::clean_cache();
        });

        return Self { 
            mods: ui_mods, 
            mc_versions, 
            selected_mc_version: selected_mc_version.clone(), 
            tx_jobs, 
            rx_events,
            tx_read_jobs,
            rx_read_events, 
            deletion_confirmation: DeletionConfirmation::None, 
            
            // Fix: Add active_modpack explicitly
            active_modpack: active_modpack.clone(), 

            status_msg: String::new(),
            current_tab: AppTab::Explorer,
            profiles_db,
            memory,
            selected_profile_name: None,
            profile_mods_pending_deletion: HashSet::new(),

            selected_modpack_ui: active_modpack,

            cached_modpacks: list_modpacks(),

            loaders: vec![
                "Fabric".to_string(), 
                "Forge".to_string(), 
                "NeoForge".to_string(),
                "Quilt".to_string(),
            ],
            selected_loader: "Fabric".to_string(),

            download_confirmation_name: None,
            download_source: DownloadSource::Explorer,
            create_profile_modal_name: None,

            active_downloads: HashMap::new(),
            
            search_state: SearchState {
                 loader: "Fabric".to_string(), // Initial default
                 version: selected_mc_version.clone(),
                 ..Default::default()
            },
            tx_search,
            rx_search,
        };
    }

    /// Carga mods de una carpeta: intenta caché primero, si no, crea placeholder y envía ReadJob.
    fn load_mods_from_folder(&mut self, folder: &std::path::Path) {
        self.mods.clear();
        if let Ok(entries) = std::fs::read_dir(folder) {
            let mut entries_vec: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            entries_vec.sort_by_key(|e| e.file_name());

            for entry in entries_vec {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("jar") {
                    let key = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    let placeholder = ModInfo {
                        key: key.clone(), name: key.clone(),
                        detected_project_id: None, confirmed_project_id: None,
                        version_local: None, version_remote: None, selected: true,
                        file_size_bytes: None, file_mtime_secs: None, depends: None,
                    };
                    self.mods.insert(key, UiModInfo {
                        inner: placeholder, status: ModStatus::Resolving, progress: 0.0,
                    });
                    let _ = self.tx_read_jobs.send(ReadJob { file_path: path });
                }
            }
        }
    }

    fn render_explorer_side(&mut self, ctx: &egui::Context) {
        SidePanel::right("selector_modpacks")
        .resizable(true)
        .default_width(200.0)
        .show(ctx, |ui| {  
            ui.add_space(6.0);  
            ui.heading("MODPACKS");
            ui.add_space(6.0);
            let modpacks = self.cached_modpacks.clone();

            if modpacks.is_empty() {
                ui.label("No hay modpacks disponibles");
            } else {
                ScrollArea::vertical().show(ui, |ui| {
                    for mp in modpacks {
                        // Determine selection state
                        let is_selected_ui = self.selected_modpack_ui.as_ref() == Some(&mp);
                        
                        // Check if active on disk
                        let is_active_disk = match PATHS.mods_folder.read_link() {
                            Ok(link) => link.ends_with(&mp),
                            Err(_) => match read_active_marker() {
                                Some(active) => active == mp,
                                None => false,
                            },
                        };

                        let label_text = if is_active_disk {
                            format!("{} (Activo)", mp)
                        } else {
                            mp.clone()
                        };

                        ui.horizontal(|ui| {
                            // Layout: Botones de acción a la derecha, botón principal llena el resto
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("🗑").on_hover_text("Eliminar modpack").clicked() {
                                    self.deletion_confirmation = DeletionConfirmation::Modpack(mp.clone());
                                }

                                // Si está seleccionado en la UI y no es el activo en disco, mostrar rayo
                                if is_selected_ui && !is_active_disk {
                                    if ui.button("⚡").on_hover_text("Activar este modpack").clicked() {
                                        self.status_msg = match change_mods(&mp) {
                                            Ok(msg) => msg,
                                            Err(e) => format!("Error: {}", e),
                                        };
                                    }
                                }
                                
                                // Rellenar resto con nombre
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    if ui.add_sized(ui.available_size(), egui::Button::new(label_text).selected(is_selected_ui)).clicked() {
                                        let target_folder = if is_selected_ui {
                                            // Deseleccionar -> Cargar instalados
                                            self.selected_modpack_ui = None;
                                            PATHS.mods_folder.clone()
                                        } else {
                                            // Seleccionar -> Cargar preview
                                            self.selected_modpack_ui = Some(mp.clone());
                                            PATHS.modpacks_folder.join(&mp)
                                        };

                                        // Usar método helper
                                        self.load_mods_from_folder(&target_folder);
                                    }
                                });
                            });
                        });
                    }
                });
            }
        });
    }

    fn render_explorer_center(&mut self, ui: &mut egui::Ui) {
        // Central Panel Content for Explorer
        let title = if let Some(mp) = &self.selected_modpack_ui {
             format!("Mods en: {}", mp)
        } else {
             "Mods Instalados (Activos)".to_string()
        };
        ui.heading(title);
        
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            if ui.button(" 🔁 ")
                .on_hover_text("Actualiza la lista.")
                .clicked() {
                    let folder = if let Some(mp) = &self.selected_modpack_ui {
                        let p = PATHS.modpacks_folder.join(mp);
                        // Asegurar que existe, por si acaso
                        if !p.exists() {
                            match std::fs::create_dir_all(&p) {
                                Ok(_) => p,
                                Err(_) => PATHS.mods_folder.clone(),
                            }
                        } else {
                            p
                        }
                    } else {
                        PATHS.mods_folder.clone()
                    };
                    
                    // Usar método helper
                    self.load_mods_from_folder(&folder);
                    self.status_msg = "Actualizando lista de mods...".to_string();
            }

            if ui.button("🔍 Buscar Mods").clicked() {
                self.search_state.open = true;
                self.search_state.source = SearchSource::Explorer;
                // Sync defaults with current selection if first open or reset?
                self.search_state.version = self.selected_mc_version.clone();
                self.search_state.loader = self.selected_loader.clone();
                
                self.search_state.results.clear();
                self.search_state.query.clear();
                self.search_state.page = 0;
            }

            if ui.button(" ⬇ ")
                .on_hover_text("Actualizar mods")
                .clicked() {
                    // Open confirmation modal with default name
                    self.download_confirmation_name = Some(format!("mods{}", self.selected_mc_version));
            }
             
            if ui.button(" 🗑 ").clicked() {
                self.deletion_confirmation = DeletionConfirmation::SelectedMods;
            }

            let all_selected = self.mods.values().all(|m| m.selected);
            if ui.button(if all_selected { "✅ Todo" } else { "⬜ Todo" }).clicked() {
                if all_selected {
                    for m in self.mods.values_mut() { m.selected = false; }                       
                } else {
                    for m in self.mods.values_mut() { m.selected = true; }
                }
            }

            if ui.button(" 💾 ")
            .on_hover_text("Crea un perfil con los mods seleccionados.")
            .clicked() {
                // Open modal instead of creating directly
                self.create_profile_modal_name = Some(String::new());
            }
        });
        ui.add_space(8.0);
        ScrollArea::vertical().show(ui, |ui| {
            let keys: Vec<String> = self.mods.keys().cloned().collect();
            for key in keys {
                if let Some(m) = self.mods.get_mut(&key) {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut m.selected, "");
                        
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(&m.name).size(16.0).strong());
                                if let Some(v) = &m.version_local {
                                    ui.label(egui::RichText::new(format!("v{}", v)).color(ui.visuals().weak_text_color()));
                                }
                                
                                // Check memory for known issues
                                if let Some(issue) = self.memory.check(&m.name) {
                                    ui.label("⚠️").on_hover_text(format!("Problema conocido: {}\nSolución: {:?}", issue.issue_description, issue.fix));
                                }
                            });

                            if let Some(deps) = &m.depends {
                                let mut loaders = Vec::new();
                                let mut others = Vec::new();
                                
                                let loader_keys = ["fabricloader", "fabric-loader", "forge", "neoforge", "quilt_loader"];
                                
                                for (k, v) in deps {
                                    let clean_ver = format_version_range(v);
                                    let clean_name = format_dep_name(k);
                                    
                                    if loader_keys.contains(&k.as_str()) {
                                        loaders.push((clean_name, clean_ver));
                                    } else if ["minecraft", "fabric-api"].contains(&k.as_str()) {
                                        others.push((clean_name, clean_ver));
                                    }
                                }
                                
                                let mut display_items = Vec::new();
                                
                                // Handle Loaders
                                if loaders.len() == 1 {
                                    let (n, v) = &loaders[0];
                                    display_items.push(format!("{} {}", n, v));
                                } else if loaders.len() > 1 {
                                    let tooltip = loaders.iter().map(|(n, v)| format!("{} {}", n, v)).collect::<Vec<_>>().join("\n");
                                    ui.label(egui::RichText::new("Multi-Loader ℹ").size(11.0).color(ui.visuals().weak_text_color()))
                                    .on_hover_text(tooltip);
                                }

                                // Add others
                                for (n, v) in others {
                                    display_items.push(format!("{} {}", n, v));
                                }

                                if !display_items.is_empty() {
                                    ui.label(
                                        egui::RichText::new(display_items.join("  |  "))
                                        .size(11.0)
                                        .color(ui.visuals().weak_text_color())
                                    );
                                }
                            }
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            match &m.status {
                                ModStatus::Idle => {
                                    // Check if we are viewing the active modpack to show Toggle Link
                                    if let Some(active) = &self.active_modpack {
                                        if let Some(selected) = &self.selected_modpack_ui {
                                            if active == selected {
                                                // Check if linked (exists in /mods)
                                                // We can check efficiently or assume state?
                                                // Let's check existence for now.
                                                let link_path = crate::paths_vars::PATHS.mods_folder.join(&m.inner.key);
                                                let is_linked = link_path.exists();

                                                if is_linked {
                                                    if ui.button("🔌").on_hover_text("Desactivar (Desvincular)").clicked() {
                                                        // Unlink
                                                        let _ = std::fs::remove_file(&link_path);
                                                    }
                                                } else {
                                                     if ui.button("⚪").on_hover_text("Activar (Vincular)").clicked() {
                                                        // Link
                                                        let source_path = crate::paths_vars::PATHS.modpacks_folder.join(selected).join(&m.inner.key);
                                                        // Try hardlink
                                                        if std::fs::hard_link(&source_path, &link_path).is_err() {
                                                            // error handling?
                                                        }
                                                     }
                                                }
                                            }
                                        }
                                    }
                                },
                                ModStatus::Resolving => { ui.label("⌛"); },
                                ModStatus::Downloading(p) => { ui.label(format!("📥 {:.0}%", p * 100.0)); },
                                ModStatus::Done => { ui.label("✅"); },
                                ModStatus::Error(msg) => { ui.label(format!("❌ {}", msg)); },
                            }
                        });
                    });
                    ui.separator();
                }
            }
        });
    }

    fn render_profiles_side(&mut self, ctx: &egui::Context) {
        SidePanel::left("profiles_list")
            .resizable(true)
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.add_space(5.0);
                
                ui.horizontal(|ui| {
                    if ui.button(" ➕ ").on_hover_text("Crear perfil").clicked() {
                         self.create_profile_modal_name = Some(String::new());
                    }
                    if ui.button(" ⬇ ").on_hover_text("Instalar/Descargar").clicked() {
                        if let Some(selected) = &self.selected_profile_name {
                             // Open download modal for profile
                             self.download_confirmation_name = Some(selected.clone());
                             self.download_source = DownloadSource::Profile(selected.clone());
                        } else {
                            self.status_msg = "Selecciona un perfil primero para descargar.".to_string();
                        }   
                    }
                });
                
                ui.separator();

                ScrollArea::vertical().show(ui, |ui| {
                    let names: Vec<String> = self.profiles_db.profiles.keys().cloned().collect();
                    for name in names {
                        ui.horizontal(|ui| {
                            let is_selected = self.selected_profile_name.as_ref() == Some(&name);
                            if ui.selectable_label(is_selected, &name).clicked() {
                                if self.selected_profile_name.as_ref() != Some(&name) {
                                    self.selected_profile_name = Some(name.clone());
                                    self.profile_mods_pending_deletion.clear();
                                }
                            }
                            if ui.button("🗑").clicked() {
                                self.deletion_confirmation = DeletionConfirmation::Profile(name.clone());
                            }
                        });
                    }
                });
            });
    }

    fn render_profiles_center(&mut self, ui: &mut egui::Ui) {
        // Main Profile Editor
        if let Some(name) = &self.selected_profile_name.clone() {
            let mut should_save = false; 
            if let Some(profile) = self.profiles_db.get_profile_mut(name) {
                ui.horizontal(|ui| {
                    ui.label("Nombre:");
                    ui.text_edit_singleline(&mut profile.name);
                    
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_sized([40.0, 40.0], egui::Button::new("💾")).on_hover_text("Guardar cambios").clicked() {
                            should_save = true;
                            self.status_msg = "Perfil guardado.".to_string();
                        }
                        ui.add_space(5.0);
                        if ui.add_sized([40.0, 40.0], egui::Button::new("🔍")).on_hover_text("Buscar / Añadir Mod").clicked() {
                             self.search_state.open = true;
                             self.search_state.source = SearchSource::Profile(name.clone());
                             
                             // Sync defaults
                             self.search_state.version = self.selected_mc_version.clone();
                             self.search_state.loader = self.selected_loader.clone();

                             self.search_state.results.clear();
                             self.search_state.query.clear();
                             self.search_state.page = 0;
                        }
                    });
                });
                
                ui.separator();
                ui.label(format!("Mods: {}", profile.mods.len()));
                
                ScrollArea::vertical().id_salt("profile_mods_scroll").show(ui, |ui| {
                    // Collect toggle actions to avoid borrowing issues in loop
                    let mut to_mark = Vec::new();
                    let mut to_unmark = Vec::new();

                    for (k, m) in &profile.mods {
                        let is_pending = self.profile_mods_pending_deletion.contains(k);
                        ui.horizontal(|ui| {
                            if is_pending {
                                ui.label(egui::RichText::new(&m.name).strikethrough().color(ui.visuals().weak_text_color()));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.add_space(15.0); // Padding from scrollbar
                                    if ui.button("↩").on_hover_text("Restaurar mod").clicked() {
                                        to_unmark.push(k.clone());
                                    }
                                });
                            } else {
                                ui.label(&m.name);
                                
                                // Show progress if downloading
                                if let Some(status) = self.active_downloads.get(k) {
                                    match status {
                                        ModStatus::Downloading(p) => {
                                            ui.add(egui::ProgressBar::new(*p).show_percentage());
                                        },
                                        ModStatus::Resolving => { ui.label("Resolviendo..."); },
                                        ModStatus::Done => { ui.label("✔"); },
                                        ModStatus::Error(e) => { ui.label(egui::RichText::new(format!("Error: {}", e)).color(egui::Color32::RED)); },
                                        _ => {}
                                    }
                                }

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.add_space(15.0); // Padding from scrollbar
                                    if ui.button("❌").on_hover_text("Marcar para borrar").clicked() {
                                        to_mark.push(k.clone());
                                    }
                                });
                            }
                        });
                    }

                    for k in to_mark { self.profile_mods_pending_deletion.insert(k); }
                    for k in to_unmark { self.profile_mods_pending_deletion.remove(&k); }
                });
            } else {
                ui.label("Perfil no encontrado (¿borrado?)");
            }

            if should_save {
                // Apply pending deletions first
                if !self.profile_mods_pending_deletion.is_empty() {
                    if let Some(p) = self.profiles_db.get_profile_mut(name) {
                        for k in &self.profile_mods_pending_deletion {
                            p.mods.shift_remove(k);
                        }
                    }
                    self.profile_mods_pending_deletion.clear();
                }

                // Check if name changed for re-keying
                let old_key = name.clone();
                // Access mutable to read new name, but we need to drop mutable borrow before modifying the map structure
                // Ideally we already modified 'profile.name' above.
                // We need to retrieve it again or clone the new name.
                let mut new_key = String::new();
                if let Some(p) = self.profiles_db.get_profile(name) {
                    new_key = p.name.clone();
                }

                if !new_key.is_empty() && old_key != new_key {
                     // Re-keying needed
                     if let Some(p) = self.profiles_db.profiles.shift_remove(&old_key) {
                         self.profiles_db.add_profile(p);
                         self.selected_profile_name = Some(new_key);
                         self.status_msg = format!("Perfil renombrado a '{}'.", self.selected_profile_name.as_deref().unwrap_or("?"));
                     }
                }

                save_profiles(&self.profiles_db);
            }
        } else {
            ui.label("Selecciona un perfil de la izquierda o crea uno nuevo.");
        }
    }
}

impl eframe::App for ModUpdaterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Top Bar (Tabs) ---
        TopBottomPanel::top("top_tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(self.current_tab == AppTab::Explorer, "📂 Mods").clicked() {
                    self.current_tab = AppTab::Explorer;
                }
                if ui.selectable_label(self.current_tab == AppTab::Profiles, "👥 Perfiles").clicked() {
                    self.current_tab = AppTab::Profiles;
                }
            });
        });

        // --- Status Bar ---
        TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status_msg);
            });
        });

        // --- Side Panels (Must be called before CentralPanel) ---
        match self.current_tab {
            AppTab::Explorer => self.render_explorer_side(ctx),
            AppTab::Profiles => self.render_profiles_side(ctx),
        }

        // --- Main Content ---
        CentralPanel::default().show(ctx, |ui| {
            match self.current_tab {
                AppTab::Explorer => self.render_explorer_center(ui),
                AppTab::Profiles => self.render_profiles_center(ui),
            }
        });

        // --- Background Events (Downloads & Reads) ---
        for ev in self.rx_events.try_iter() {
            match ev {
                // ... (Download events handling) ...
                // Keep existing logic for self.mods but also update active_downloads
                DownloadEvent::Resolving { key } => {
                    self.active_downloads.insert(key.clone(), ModStatus::Resolving);
                    if let Some(m) = self.mods.get_mut(&key) { m.status = ModStatus::Resolving; }
                }
                DownloadEvent::Resolved { key } => {
                     // Maybe idle or keep resolving?
                     if let Some(m) = self.mods.get_mut(&key) { m.status = ModStatus::Idle; }
                }
                DownloadEvent::Started { key } => {
                    self.active_downloads.insert(key.clone(), ModStatus::Downloading(0.0));
                    if let Some(m) = self.mods.get_mut(&key) { m.status = ModStatus::Downloading(0.0); }
                }
                DownloadEvent::Progress(key, p) => {
                    self.active_downloads.insert(key.clone(), ModStatus::Downloading(p));
                    if let Some(m) = self.mods.get_mut(&key) { m.status = ModStatus::Downloading(p); m.progress = p; }
                }
                DownloadEvent::ResolvedInfo { key, confirmed_project_id, version_remote } => {
                    if let Some(m) = self.mods.get_mut(&key) {
                        m.inner.confirmed_project_id = confirmed_project_id.clone();
                        m.inner.version_remote = version_remote.clone();
                        // Save cache (Redb Update)
                        crate::manage_mods::cache::update_remote_info(&key, confirmed_project_id, version_remote);
                    }
                }
                DownloadEvent::Done { key } => {
                    self.active_downloads.insert(key.clone(), ModStatus::Done);
                    if let Some(m) = self.mods.get_mut(&key) { m.status = ModStatus::Done; m.progress = 1.0; }
                }
                DownloadEvent::Error { key, msg } => {
                     self.active_downloads.insert(key.clone(), ModStatus::Error(msg.clone()));
                     // Log error to status if urgent, otherwise just mark mod
                     if let Some(m) = self.mods.get_mut(&key) { m.status = ModStatus::Error(msg); }
                }
            }
        }
        
        for ev in self.rx_read_events.try_iter() {
            match ev {
                ReadEvent::Done { info } => {
                    let key = info.key.clone();
                    if let Some(placeholder) = self.mods.get_mut(&key) {
                        placeholder.inner = info;
                        placeholder.status = ModStatus::Idle;
                    } else {
                        // Edge case: Mod wasn't in placeholder (maybe new file appeared?)
                        // Insert it now
                        self.mods.insert(key, UiModInfo::from(info));
                    }
                }
                ReadEvent::Error { path, msg } => {
                    println!("Error reading {:?}: {}", path, msg);
                    // Optionally update status
                    let key = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    if let Some(m) = self.mods.get_mut(&key) {
                        m.status = ModStatus::Error("Failed to read".to_string());
                    }
                }
            }
        }

        // --- Global Deletion Modal ---
        if self.deletion_confirmation != DeletionConfirmation::None {
            egui::Window::new("Confirmar Acción")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    match &self.deletion_confirmation {
                        DeletionConfirmation::Modpack(name) => { ui.label(format!("¿Borrar modpack '{}' de disco?", name)); },
                        DeletionConfirmation::SelectedMods => {
                             if let Some(mp) = &self.selected_modpack_ui {
                                 if self.active_modpack.as_ref() == Some(mp) {
                                     ui.label(egui::RichText::new("⚠ MODPACK ACTIVO").color(egui::Color32::YELLOW).strong());
                                     ui.label("Se eliminarán los mods del modpack Y también del juego (hardlinks).");
                                 } else {
                                     ui.label("¿Borrar mods seleccionados del modpack?");
                                 }
                             } else {
                                 ui.label("¿Borrar mods seleccionados de disco?");
                             }
                        },
                        DeletionConfirmation::Profile(name) => { ui.label(format!("¿Borrar perfil lógico '{}'? (No borra archivos)", name)); },
                        DeletionConfirmation::None => { ui.label(""); },
                    };
                    
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Cancelar").clicked() {
                            self.deletion_confirmation = DeletionConfirmation::None;
                        }
                        if ui.button("Confirmar").clicked() {
                            match self.deletion_confirmation.clone() {
                                DeletionConfirmation::Modpack(name) => {
                                    let target = PATHS.modpacks_folder.join(&name);
                                    if std::fs::remove_dir_all(&target).is_ok() {
                                        self.status_msg = format!("Modpack '{}' eliminado.", name);
                                        self.cached_modpacks = list_modpacks();
                                    }
                                }
                                DeletionConfirmation::SelectedMods => {
                                    let target_folder = if let Some(mp) = &self.selected_modpack_ui {
                                        PATHS.modpacks_folder.join(mp)
                                    } else {
                                        PATHS.mods_folder.clone()
                                    };
                                    
                                    // Check if we need dual deletion (Active Modpack)
                                    let is_active = self.selected_modpack_ui.is_some() && self.active_modpack == self.selected_modpack_ui;

                                    let mut removed_cnt = 0;
                                    // Clonamos para evitar problemas de borrow checker al iterar y modificar
                                    let keys_to_remove: Vec<String> = self.mods.iter()
                                        .filter(|(_, m)| m.selected)
                                        .map(|(k, _)| k.clone())
                                        .collect();

                                    for key in keys_to_remove {
                                        // 1. Delete source
                                        let path = target_folder.join(&key);
                                        if std::fs::remove_file(&path).is_ok() {
                                            self.mods.shift_remove(&key);
                                            removed_cnt += 1;
                                            
                                            // 2. Delete hardlink if active
                                            if is_active {
                                                let link_path = PATHS.mods_folder.join(&key);
                                                let _ = std::fs::remove_file(link_path);
                                            }
                                        }
                                    }
                                    self.status_msg = format!("{} mods eliminados.", removed_cnt);
                                }
                                DeletionConfirmation::Profile(name) => {
                                    self.profiles_db.delete_profile(&name);
                                    save_profiles(&self.profiles_db);
                                    if self.selected_profile_name.as_ref() == Some(&name) {
                                        self.selected_profile_name = None;
                                    }
                                    self.status_msg = format!("Perfil '{}' eliminado.", name);
                                }
                                _ => {}
                            }
                            self.deletion_confirmation = DeletionConfirmation::None;
                        }
                    });
                });
        }

        if let Some(name_rc) = &self.download_confirmation_name.clone() {
            let mut name = name_rc.clone();
            let mut open = true;
            // ... (rest of download modal logic, just anchoring next modal after this block)
            // Note: Since I can't see the end of the download modal in the previous view, I will append the NEW modal at the end of the update loop, 
            // but I need to be careful about where I insert it.
            // Actually, I'll insert it right before the download confirmation modal or after it. 
            // The previous view_file didn't show the end of the download confirmation modal.
            // I will use a different anchor.

            let mut close_requested = false;

            egui::Window::new("Confirmar Descarga")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label("Nombre de la carpeta del Modpack:");
                    ui.text_edit_singleline(&mut name);
                    
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(5.0);
                    
                    // 1. Selector de Loader
                    ui.horizontal(|ui| {
                        ui.label("Loader:");
                        egui::ComboBox::from_id_salt("loader-selector-modal")
                            .selected_text(&self.selected_loader)
                            .show_ui(ui, |ui| {
                                for loader in &self.loaders {
                                    ui.selectable_value(&mut self.selected_loader, loader.clone(), loader);
                                }
                            });

                        // 2. Selector de Versión MC
                        ui.label("Versión:");
                        egui::ComboBox::from_id_salt("mc-version-box-modal")
                            .selected_text(&self.selected_mc_version)
                            .show_ui(ui, |ui| {
                                for v in &self.mc_versions {
                                    ui.selectable_value(&mut self.selected_mc_version, v.clone(), v);
                                }
                            ui.separator();
                            ui.text_edit_singleline(&mut self.selected_mc_version);
                        });
                    });

                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        if ui.button("Cancelar").clicked() {
                            close_requested = true;
                        }
                        if ui.button("Confirmar").clicked() {
                            if !name.trim().is_empty() {
                                // Logic to start download
                                let output_folder_path = PATHS.modpacks_folder.join(&name);
                                // Create dir
                                let _ = std::fs::create_dir_all(&output_folder_path);
        
                                let mut count = 0;

                                match &self.download_source {
                                    DownloadSource::Explorer => {
                                        for (k, m) in self.mods.clone().into_iter() {
                                            if m.selected {
                                                let job = DownloadJob {
                                                    key: k.clone(),
                                                    modinfo: m.inner.clone(),
                                                    output_folder: output_folder_path.to_string_lossy().to_string(),
                                                    selected_version: self.selected_mc_version.clone(),
                                                    selected_loader: self.selected_loader.clone(),
                                                };
                                                let _ = self.tx_jobs.send(job);
                                                count += 1;
                                            }
                                        }
                                    },
                                    DownloadSource::Profile(profile_name) => {
                                        if let Some(profile) = self.profiles_db.get_profile(profile_name) {
                                            for (k, m) in &profile.mods {
                                                let job = DownloadJob {
                                                    key: k.clone(),
                                                    modinfo: m.clone(),
                                                    output_folder: output_folder_path.to_string_lossy().to_string(),
                                                    selected_version: self.selected_mc_version.clone(),
                                                    selected_loader: self.selected_loader.clone(),
                                                };
                                                let _ = self.tx_jobs.send(job);
                                                count += 1;
                                            }
                                        }
                                    }
                                }
                                self.status_msg = format!("Iniciando descarga de {} mods en '{}'", count, name);
                                self.cached_modpacks = list_modpacks();
                                close_requested = true;
                            }
                        }
                    });
                });
            
            // If window closed via X button or logic request
            if !open || close_requested {
                self.download_confirmation_name = None;
            } else {
                // write back changes to text field
                self.download_confirmation_name = Some(name);
            }
        }

        // --- Create Profile Modal ---
        if let Some(name_rc) = &self.create_profile_modal_name.clone() {
            let mut name = name_rc.clone();
            let mut open = true;
            let mut close_requested = false;

            egui::Window::new("Crear Perfil")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label("Nombre del Perfil:");
                    ui.text_edit_singleline(&mut name);
                    ui.add_space(10.0);
                    
                    ui.horizontal(|ui| {
                        if ui.button("Cancelar").clicked() {
                            close_requested = true;
                        }
                        if ui.button("Guardar").clicked() {
                            if !name.trim().is_empty() {
                                if self.current_tab == AppTab::Explorer {
                                    // Create from selected mods
                                    let mods_map: IndexMap<String, ModInfo> = self.mods.iter().map(|(k, v)| (k.clone(), v.inner.clone())).collect();
                                    let mut profile = Profile::new(name.clone(), Some("Importado desde carpeta".to_string()));
                                    profile.mods = mods_map;
                                    self.profiles_db.add_profile(profile);
                                    save_profiles(&self.profiles_db);
                                    self.status_msg = format!("Perfil '{}' creado (con mods).", name);
                                } else {
                                    // Create empty profile (Profiles Tab)
                                    let profile = Profile::new(name.clone(), None);
                                    self.profiles_db.add_profile(profile);
                                    save_profiles(&self.profiles_db);
                                    self.status_msg = format!("Perfil '{}' creado.", name);
                                }
                                close_requested = true;
                            }
                        }
                    });
                });

             // If window closed via X button or logic request
            if !open || close_requested {
                self.create_profile_modal_name = None;
            } else {
                // write back changes to text field
                self.create_profile_modal_name = Some(name);
            }
        }

        // --- Search Modal ---
        self.render_search_modal(ctx);

        // --- Search Results Poll ---
        while let Ok((new_results, _source, offset)) = self.rx_search.try_recv() {
            if offset == 0 {
                self.search_state.results = new_results;
            } else {
                self.search_state.results.extend(new_results);
            }
            self.search_state.is_searching = false;
        }
    }
}

impl ModUpdaterApp {
    fn render_search_modal(&mut self, ctx: &egui::Context) {
        let mut open = self.search_state.open;
        if !open { return; }

         let title = if let SearchSource::Profile(p) = &self.search_state.source {
             format!("🔍 Buscar Mods para '{}'", p)
         } else {
             "🔍 Buscar Mods (Descargar)".to_string()
         };

        egui::Window::new(&title)
            .open(&mut open)
            .resize(|r| r.fixed_size(egui::vec2(700.0, 600.0))) // Start larger
            .show(ctx, |ui| {
                // Filters Row (Only for Explorer / Direct Download)
                let is_explorer = matches!(self.search_state.source, SearchSource::Explorer);
                
                if is_explorer {
                    ui.horizontal(|ui| {
                        ui.label("Loader:");
                        egui::ComboBox::from_id_salt("search_loader")
                            .selected_text(&self.search_state.loader)
                            .show_ui(ui, |ui| {
                                for l in &self.loaders {
                                    ui.selectable_value(&mut self.search_state.loader, l.clone(), l);
                                }
                            });

                        ui.label("Versión:");
                        egui::ComboBox::from_id_salt("search_version_selector")
                            .selected_text(&self.search_state.version)
                            .show_ui(ui, |ui| {
                                for v in &self.mc_versions {
                                    ui.selectable_value(&mut self.search_state.version, v.clone(), v);
                                }
                            });
                    });
                    ui.add_space(5.0);
                }

                ui.horizontal(|ui| {
                    ui.label("Buscar:");
                    let text_box = ui.text_edit_singleline(&mut self.search_state.query);
                    
                    let mut do_search = false;

                    if text_box.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                         do_search = true;
                    }
                    if ui.button("Buscar").clicked() {
                         do_search = true;
                    }

                    if do_search {
                        if !self.search_state.query.trim().is_empty() {
                            self.search_state.is_searching = true;
                            self.search_state.page = 0;
                            self.search_state.results.clear();
                            
                            let (loader, version) = if is_explorer {
                                (Some(self.search_state.loader.clone()), Some(self.search_state.version.clone()))
                            } else {
                                (None, None)
                            };

                            let req = SearchRequest {
                                query: self.search_state.query.clone(),
                                loader,
                                version,
                                offset: 0,
                                limit: self.search_state.limit,
                            };
                            let _ = self.tx_search.send((req, self.search_state.source.clone()));
                        }
                    }
                });
                
                if self.search_state.is_searching && self.search_state.page == 0 {
                    ui.spinner();
                    ui.label("Buscando en Modrinth y CurseForge...");
                }

                ui.separator();

                egui::ScrollArea::vertical().max_height(450.0).show(ui, |ui| {
                    for res in &self.search_state.results {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.heading(&res.name);
                                    ui.label(egui::RichText::new(&res.author).weak());
                                    // Use small truncate or multiline?
                                    ui.label(egui::RichText::new(&res.description).weak().size(10.0));
                                    
                                    ui.horizontal(|ui| {
                                        if res.modrinth_id.is_some() { ui.label(egui::RichText::new("Modrinth").color(egui::Color32::GREEN)); }
                                        if res.curseforge_id.is_some() { ui.label(egui::RichText::new("CurseForge").color(egui::Color32::ORANGE)); }
                                    });
                                });

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    // Contextual Action
                                    match &self.search_state.source {
                                        SearchSource::Explorer => {
                                            // Check progress by name
                                            if let Some(status) = self.active_downloads.get(&res.name) {
                                                match status {
                                                    ModStatus::Downloading(p) => { ui.add(egui::ProgressBar::new(*p).show_percentage().animate(true)); },
                                                    ModStatus::Resolving => { ui.spinner(); ui.label("Resolviendo..."); },
                                                    ModStatus::Done => { ui.label("✔ Instalado"); },
                                                    ModStatus::Error(e) => { ui.colored_label(egui::Color32::RED, "Error"); ui.label(e); },
                                                    _ => {}
                                                }
                                            } else {
                                                if ui.button("📥 Descargar").clicked() {
                                                    // Trigger Download
                                                    // Construct ModInfo
                                                    let mod_info = crate::manage_mods::ModInfo {
                                                        key: res.name.clone(), 
                                                        name: res.name.clone(),
                                                        detected_project_id: res.modrinth_id.clone().or_else(|| Some(res.slug.clone())),
                                                        confirmed_project_id: res.modrinth_id.clone().or_else(|| res.curseforge_id.map(|id| id.to_string())),
                                                        version_local: Some("".to_string()),
                                                        version_remote: None,
                                                        selected: true,
                                                        file_size_bytes: None,
                                                        file_mtime_secs: None,
                                                        depends: None,
                                                    };
                                                    
                                                    // Output folder
                                                    let output_folder_path = if let Some(mp) = &self.selected_modpack_ui {
                                                        PATHS.modpacks_folder.join(mp)
                                                    } else {
                                                        crate::manage_mods::prepare_output_folder(&self.selected_mc_version);
                                                        PATHS.modpacks_folder.join(format!(r"mods{}", self.selected_mc_version))
                                                    };
                                                    let _ = std::fs::create_dir_all(&output_folder_path);

                                                    let job = DownloadJob {
                                                        key: res.name.clone(),
                                                        modinfo: mod_info.clone(),
                                                        output_folder: output_folder_path.to_string_lossy().to_string(),
                                                        selected_version: self.search_state.version.clone(), // Use search version
                                                        selected_loader: self.search_state.loader.clone(), // Use search loader
                                                    };
                                                    let _ = self.tx_jobs.send(job);
                                                    
                                                    // Mark as resolving locally
                                                    self.active_downloads.insert(res.name.clone(), ModStatus::Resolving);
                                                }
                                            }
                                        },
                                        SearchSource::Profile(p_name) => {
                                            if ui.button("➕ Añadir").clicked() {
                                                if let Some(profile) = self.profiles_db.get_profile_mut(p_name) {
                                                    // Add to profile
                                                    let mod_info = crate::manage_mods::ModInfo {
                                                    key: res.name.clone(),
                                                    name: res.name.clone(),
                                                    detected_project_id: res.modrinth_id.clone(),
                                                    confirmed_project_id: res.modrinth_id.clone().or_else(|| res.curseforge_id.map(|id| id.to_string())),
                                                    version_local: Some("Universal".to_string()), 
                                                    version_remote: None,
                                                    selected: true,
                                                    file_size_bytes: None,
                                                    file_mtime_secs: None,
                                                    depends: None,
                                                };
                                                profile.mods.insert(res.name.clone(), mod_info);
                                                }
                                                // save_profiles(&self.profiles_db); // Auto-save happens in update logic for profiles? No, explicits.
                                                // We should save here.
                                                save_profiles(&self.profiles_db);
                                                self.status_msg = format!("Mod '{}' añadido al perfil.", res.name);
                                            }
                                        }
                                    }
                                });
                            });
                        });
                    }
                    
                    if !self.search_state.results.is_empty() {
                        ui.add_space(10.0);
                        if self.search_state.is_searching {
                            ui.spinner();
                        } else {
                            if ui.button("⬇ Cargar más resultados").clicked() {
                                self.search_state.is_searching = true;
                                self.search_state.page += 1;
                                let offset = self.search_state.page * self.search_state.limit;
                                
                                let is_explorer = matches!(self.search_state.source, SearchSource::Explorer);
                                let (loader, version) = if is_explorer {
                                    (Some(self.search_state.loader.clone()), Some(self.search_state.version.clone()))
                                } else {
                                    (None, None)
                                };

                                let req = SearchRequest {
                                    query: self.search_state.query.clone(),
                                    loader,
                                    version,
                                    offset,
                                    limit: self.search_state.limit,
                                };
                                let _ = self.tx_search.send((req, self.search_state.source.clone()));
                            }
                        }
                    }
                });

            });
        
        self.search_state.open = open;
    } 
}
