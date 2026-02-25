use eframe::{egui, egui::CentralPanel};
use std::collections::{HashSet, HashMap};
use eframe::egui::{ScrollArea, SidePanel, TopBottomPanel};
use std::ops::{Deref, DerefMut};
use crossbeam_channel::{unbounded, Sender, Receiver};
use std::thread;
use indexmap::IndexMap;

use crate::local_mods_ops::{
    change_mods, list_modpacks, get_minecraft_versions, read_active_marker,
    spawn_read_workers, ReadJob, ReadEvent,
    ModInfo,
};
use crate::local_datapacks_ops::{
    DatapackInfo, DatapackReadJob, DatapackReadEvent,
    list_worlds, spawn_datapack_read_workers, pack_format_to_mc,
};
use crate::profiles::{ProfilesDatabase, Profile, load_profiles, save_profiles};
use crate::fetch::async_download::{spawn_workers, DownloadJob, DownloadEvent};
use crate::fetch::single_mod_search::{UnifiedSearchResult, search_unified, SearchRequest};
use crate::paths_vars::PATHS;
use super::utils::{format_dep_name, format_version_range};
use super::tui_theme::{self, tui_button, tui_button_c, tui_checkbox, tui_heading, tui_separator, tui_tab, tui_dim, tui_number};

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
    pub download_dependencies: bool,
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
            download_dependencies: true,
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
    Datapacks,
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

    // --- Dependency Resolution State ---
    // (mod_key, project_id, version, loader, output_folder, existing_project_ids, existing_filenames)
    tx_resolve_deps: Sender<(String, String, String, String, String, std::collections::HashSet<String>, std::collections::HashSet<String>)>,
    // Returns (mod_key, list_of_dep_display_names) so the search card can update
    rx_dep_resolved: Receiver<(String, Vec<String>)>,

    // --- Profile Dependency Resolution ---
    // (project_id, profile_name, version, loader, existing_project_ids)
    tx_resolve_profile_deps: Sender<(String, String, String, String, std::collections::HashSet<String>)>,
    // Returns (profile_name, Vec<(dep_name, dep_filename, dep_project_id, dep_slug)>)
    rx_profile_deps_resolved: Receiver<(String, Vec<(String, String, String, String)>)>,

    // --- Search Dep-Name Preview Resolution ---
    // (slug, project_id, version, loader, cf_key)
    tx_fetch_dep_names: Sender<(String, String, String, String, String)>,
    // Returns (slug, dep_names)
    rx_dep_names_result: Receiver<(String, Vec<String>)>,

    // --- Datapacks State ---
    pub cached_worlds: Vec<String>,
    pub world_datapacks: IndexMap<String, IndexMap<String, DatapackInfo>>,
    tx_dp_read_jobs: Sender<DatapackReadJob>,
    rx_dp_read_events: Receiver<DatapackReadEvent>,
    datapacks_loaded: bool,
}

impl ModUpdaterApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Aplicar tema TUI
        tui_theme::apply_tui_theme(&_cc.egui_ctx);

        // Detect active modpack first to use in logic
        let active_modpack = crate::local_mods_ops::fs_ops::read_active_marker();

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
                            (meta.len(), crate::local_mods_ops::scanner::get_file_mtime(&meta))
                        } else { (0, 0) };

                        let mut loaded = false;
                        if let Some(cached) = crate::local_mods_ops::cache::get_mod(&filename) {
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

        // --- Background Cache Cleanup ---
        thread::spawn(|| {
            // Wait a bit to let the app load critical stuff first
            thread::sleep(std::time::Duration::from_secs(5));
            crate::local_mods_ops::cache::clean_cache();
        });

        // --- Dependency Resolver ---
        // Channel: UI → Resolver
        let (tx_resolve_deps, rx_resolve_deps) = unbounded::<(String, String, String, String, String, std::collections::HashSet<String>, std::collections::HashSet<String>)>();
        // Channel: Resolver → UI (dep names discovered, to update search card)
        let (tx_dep_resolved, rx_dep_resolved) = unbounded::<(String, Vec<String>)>();

        {
            let tx_jobs_clone = tx_jobs.clone();
            let tx_dep_res = tx_dep_resolved.clone();
            thread::spawn(move || {
                while let Ok((mod_key, project_id, version, loader, output_folder, existing_ids, existing_filenames)) = rx_resolve_deps.recv() {
                    println!("🔍 Resolviendo dependencias transitivas para {}...", mod_key);
                    let cf_key = crate::fetch::cf_api_key();

                    let dep_infos = crate::fetch::fetch_from_api::resolve_all_dependencies(
                        &project_id,
                        &version,
                        &loader,
                        &cf_key,
                        &existing_ids,
                    );

                    // Filter out deps whose filename is already in the modpack (installed but not cached)
                    let dep_infos: Vec<_> = dep_infos.into_iter().filter(|dep| {
                        if existing_filenames.contains(&dep.filename) {
                            println!("⏸ {} ya existe en el modpack (filename match), saltando.", dep.filename);
                            false
                        } else {
                            true
                        }
                    }).collect();

                    // Collect display names to send back to UI
                    let dep_names: Vec<String> = dep_infos.iter().map(|d| d.filename.clone()).collect();
                    let _ = tx_dep_res.send((mod_key.clone(), dep_names));

                    // Enqueue each dep as a normal DownloadJob so it shows in DESCARGA
                    for dep_info in dep_infos {
                        println!("📥 Encolando dependencia: {}", dep_info.filename);
                        let dep_mod_info = crate::local_mods_ops::ModInfo {
                            key: dep_info.filename.clone(),
                            name: dep_info.filename.clone(),
                            detected_project_id: Some(dep_info.project_id.clone()),
                            confirmed_project_id: Some(dep_info.project_id.clone()),
                            version_local: Some(version.clone()),
                            version_remote: Some(dep_info.version_remote.clone()),
                            selected: true,
                            file_size_bytes: None,
                            file_mtime_secs: None,
                            depends: None,
                        };
                        let job = crate::fetch::async_download::DownloadJob {
                            key: dep_info.filename.clone(),
                            modinfo: dep_mod_info,
                            output_folder: output_folder.clone(),
                            selected_version: version.clone(),
                            selected_loader: loader.clone(),
                        };
                        let _ = tx_jobs_clone.send(job);
                    }
                }
            });
        }

        // --- Profile Dependency Resolver ---
        let (tx_resolve_profile_deps, rx_resolve_profile_deps) = unbounded::<(String, String, String, String, std::collections::HashSet<String>)>();
        let (tx_profile_deps_res, rx_profile_deps_resolved) = unbounded::<(String, Vec<(String, String, String, String)>)>();

        {
            thread::spawn(move || {
                while let Ok((project_id, profile_name, version, loader, existing_ids)) = rx_resolve_profile_deps.recv() {
                    println!("🔍 Resolviendo deps para perfil '{}' (mod: {})...", profile_name, project_id);
                    let cf_key = crate::fetch::cf_api_key();

                    let dep_infos = crate::fetch::fetch_from_api::resolve_all_dependencies(
                        &project_id,
                        &version,
                        &loader,
                        &cf_key,
                        &existing_ids,
                    );

                    // Send (name, filename, project_id, slug) so the UI can deduplicate against slug-based detected_project_id
                    let deps: Vec<(String, String, String, String)> = dep_infos.into_iter()
                        .map(|d| (d.name, d.filename, d.project_id, d.slug))
                        .collect();

                    let _ = tx_profile_deps_res.send((profile_name, deps));
                }
            });
        }

        // --- Search Dep-Name Preview Worker Pool ---
        let (tx_fetch_dep_names, rx_fetch_dep_names) = unbounded::<(String, String, String, String, String)>();
        let (tx_dep_names_res, rx_dep_names_result) = unbounded::<(String, Vec<String>)>();
        {
            let dep_workers = crate::ui::utils::calculate_worker_count(10);
            let tx_res = std::sync::Arc::new(tx_dep_names_res);
            crate::common::spawn_worker_pool(dep_workers, rx_fetch_dep_names, move |job: (String, String, String, String, String)| {
                let (slug, project_id, version, loader, cf_key) = job;
                let names = crate::fetch::fetch_from_api::fetch_dependency_names(&project_id, &version, &loader, &cf_key);
                let _ = tx_res.send((slug, names));
            });
        }

        // --- Datapack Read Workers ---
        let (tx_dp_read_jobs, rx_dp_read_jobs) = unbounded::<DatapackReadJob>();
        let (tx_dp_read_events_send, rx_dp_read_events) = unbounded::<DatapackReadEvent>();
        {
            let dp_workers = crate::ui::utils::calculate_worker_count(10);
            spawn_datapack_read_workers(dp_workers, rx_dp_read_jobs, tx_dp_read_events_send);
        }

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
            selected_profile_name: None,
            profile_mods_pending_deletion: HashSet::new(),

            selected_modpack_ui: active_modpack,

            cached_modpacks: list_modpacks(),

            loaders: vec![
                "Fabric".to_string(), 
                "Forge".to_string(), 
                "NeoForge".to_string(),
                "Quilt".to_string(),
                "LiteLoader".to_string(),
                "Cauldron".to_string(),
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
            tx_resolve_deps,
            rx_dep_resolved,
            tx_resolve_profile_deps,
            rx_profile_deps_resolved,
            tx_fetch_dep_names,
            rx_dep_names_result,

            // Datapacks
            cached_worlds: Vec::new(),
            world_datapacks: IndexMap::new(),
            tx_dp_read_jobs,
            rx_dp_read_events,
            datapacks_loaded: false,
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
            ui.add_space(4.0);
            tui_heading(ui, "MODPACKS");
            ui.add_space(4.0);
            let modpacks = self.cached_modpacks.clone();

            if modpacks.is_empty() {
                tui_dim(ui, "(vacio)");
            } else {
                ScrollArea::vertical().show(ui, |ui| {
                    for mp in modpacks {
                        let is_selected_ui = self.selected_modpack_ui.as_ref() == Some(&mp);
                        
                        let is_active_disk = match PATHS.mods_folder.read_link() {
                            Ok(link) => link.ends_with(&mp),
                            Err(_) => match read_active_marker() {
                                Some(active) => active == mp,
                                None => false,
                            },
                        };

                        let indicator = if is_active_disk && is_selected_ui {
                            ">> " 
                        } else if is_selected_ui {
                            "> "
                        } else {
                            "  "
                        };

                        let suffix = if is_active_disk { " [ON]" } else { "" };

                        ui.horizontal(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if tui_button_c(ui, "X", tui_theme::NEON_RED).on_hover_text("Eliminar modpack").clicked() {
                                    self.deletion_confirmation = DeletionConfirmation::Modpack(mp.clone());
                                }

                                if is_selected_ui && !is_active_disk {
                                    if tui_button_c(ui, "OFF", tui_theme::NEON_RED).on_hover_text("Activar este modpack").clicked() {
                                        self.status_msg = match change_mods(&mp) {
                                            Ok(msg) => msg,
                                            Err(e) => format!("Error: {}", e),
                                        };
                                    }
                                }

                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    let label = format!("{}{}{}", indicator, mp, suffix);
                                    let color = if is_selected_ui { tui_theme::ACCENT } else { tui_theme::TEXT_PRIMARY };
                                    let text = egui::RichText::new(&label).color(color).family(egui::FontFamily::Monospace);
                                    if ui.add_sized(ui.available_size(), egui::Button::new(text)
                                        .fill(egui::Color32::TRANSPARENT)
                                        .stroke(egui::Stroke::NONE)
                                        .corner_radius(egui::CornerRadius::ZERO)).clicked() {
                                        let target_folder = if is_selected_ui {
                                            self.selected_modpack_ui = None;
                                            PATHS.mods_folder.clone()
                                        } else {
                                            self.selected_modpack_ui = Some(mp.clone());
                                            PATHS.modpacks_folder.join(&mp)
                                        };
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
        let title = if let Some(mp) = &self.selected_modpack_ui {
            format!("MODS EN: {}", mp.to_uppercase())
        } else {
            "MODS INSTALADOS (ACTIVOS)".to_string()
        };
        tui_heading(ui, &title);
        
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            if tui_button(ui, "F5")
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

            if tui_button(ui, "BUSCAR").clicked() {
                self.search_state.open = true;
                self.search_state.source = SearchSource::Explorer;
                // Sync defaults with current selection if first open or reset?
                self.search_state.version = self.selected_mc_version.clone();
                self.search_state.loader = self.selected_loader.clone();
                
                self.search_state.results.clear();
                self.search_state.query.clear();
                self.search_state.page = 0;
            }

            if tui_button_c(ui, "↓", tui_theme::NEON_YELLOW)
                .on_hover_text("Actualizar mods")
                .clicked() {
                    // Open confirmation modal with default name
                    self.download_confirmation_name = Some(format!("mods{}", self.selected_mc_version));
            }
             
            if tui_button_c(ui, "DEL", tui_theme::NEON_RED).clicked() {
                self.deletion_confirmation = DeletionConfirmation::SelectedMods;
            }

            let all_selected = self.mods.values().all(|m| m.selected);
            if tui_button(ui, if all_selected { "x ALL" } else { "o ALL" }).clicked() {
                if all_selected {
                    for m in self.mods.values_mut() { m.selected = false; }                       
                } else {
                    for m in self.mods.values_mut() { m.selected = true; }
                }
            }

            if tui_button_c(ui, "SAVE", tui_theme::NEON_GREEN)
            .on_hover_text("Crea un perfil con los mods seleccionados.")
            .clicked() {
                // Open modal instead of creating directly
                self.create_profile_modal_name = Some(String::new());
            }
        });
        ui.add_space(8.0);
        ScrollArea::vertical().show(ui, |ui| {
            let mut keys: Vec<String> = self.mods.keys().cloned().collect();
            keys.sort_by(|a, b| {
                let na = self.mods.get(a).map(|m| m.name.to_lowercase()).unwrap_or_default();
                let nb = self.mods.get(b).map(|m| m.name.to_lowercase()).unwrap_or_default();
                na.cmp(&nb)
            });
            for key in keys {
                if let Some(m) = self.mods.get_mut(&key) {
                    ui.horizontal(|ui| {
                        tui_checkbox(ui, &mut m.selected);
                        
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(&m.name)
                                    .family(egui::FontFamily::Monospace)
                                    .color(tui_theme::TEXT_PRIMARY)
                                    .strong());
                                if let Some(v) = &m.version_local {
                                    ui.label(egui::RichText::new(format!("v{}", v))
                                        .family(egui::FontFamily::Monospace)
                                        .color(tui_theme::TEXT_DIM)
                                        .size(11.0));
                                }


                            });

                            if let Some(deps) = &m.depends {
                                let loader_keys = ["fabricloader", "fabric-loader", "forge", "neoforge", "quilt_loader"];
                                let system_keys = ["minecraft", "java"];

                                let mut loader_labels: Vec<String> = Vec::new();
                                let mut mc_label: Option<String> = None;
                                let mut mod_deps: Vec<String> = Vec::new();

                                for (k, v) in deps {
                                    if k == "java" { continue; } // silently omit
                                    let clean_ver = format_version_range(v);
                                    let clean_name = format_dep_name(k);
                                    if loader_keys.contains(&k.as_str()) {
                                        loader_labels.push(format!("{} {}", clean_name, clean_ver));
                                    } else if k == "minecraft" {
                                        mc_label = Some(format!("MC {}", clean_ver));
                                    } else if !system_keys.contains(&k.as_str()) {
                                        mod_deps.push(format!("{} {}", clean_name, clean_ver));
                                    }
                                }

                                // Build the small summary line: e.g. "Fabric >=0.14  |  MC >=1.20"
                                let mut summary_parts: Vec<String> = Vec::new();
                                match loader_labels.len() {
                                    0 => {}
                                    1 => summary_parts.push(loader_labels[0].clone()),
                                    _ => summary_parts.push(format!("Multi-Loader ({})", loader_labels.join(", "))),
                                }
                                if let Some(mc) = mc_label { summary_parts.push(mc); }

                                if !summary_parts.is_empty() {
                                    tui_dim(ui, &format!("├── {}", summary_parts.join("  |  ")));
                                }

                                // Collapsible for actual mod dependencies only
                                if !mod_deps.is_empty() {
                                    let header_text = format!("Dependencias ({})", mod_deps.len());
                                    egui::CollapsingHeader::new(
                                        egui::RichText::new(&header_text)
                                            .family(egui::FontFamily::Monospace)
                                            .color(tui_theme::TEXT_DIM)
                                            .size(11.0)
                                    )
                                    .id_salt(format!("moddeps_{}", &m.inner.key))
                                    .default_open(false)
                                    .show(ui, |ui| {
                                        for item in &mod_deps {
                                            ui.horizontal(|ui| {
                                                tui_dim(ui, "•");
                                                ui.label(
                                                    egui::RichText::new(item)
                                                        .family(egui::FontFamily::Monospace)
                                                        .color(tui_theme::TEXT_DIM)
                                                        .size(11.0)
                                                );
                                            });
                                        }
                                    });
                                }
                            }

                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            match &m.status {
                                ModStatus::Idle => {
                                    if let Some(active) = &self.active_modpack {
                                        if let Some(selected) = &self.selected_modpack_ui {
                                            if active == selected {
                                                let link_path = crate::paths_vars::PATHS.mods_folder.join(&m.inner.key);
                                                let is_linked = link_path.exists();

                                                if is_linked {
                                                    if tui_button_c(ui, "ON", tui_theme::NEON_GREEN).on_hover_text("Desactivar").clicked() {
                                                        let _ = std::fs::remove_file(&link_path);
                                                    }
                                                } else {
                                                     if tui_button(ui, "--").on_hover_text("Activar").clicked() {
                                                        let source_path = crate::paths_vars::PATHS.modpacks_folder.join(selected).join(&m.inner.key);
                                                        if std::fs::hard_link(&source_path, &link_path).is_err() {}
                                                     }
                                                }
                                            }
                                        }
                                    }
                                },
                                ModStatus::Resolving | ModStatus::Downloading(_) | ModStatus::Done | ModStatus::Error(_) => {
                                    // Download progress is shown in the DESCARGA window
                                },
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
                ui.add_space(4.0);
                
                ui.horizontal(|ui| {
                    if tui_button_c(ui, "+", tui_theme::NEON_GREEN).on_hover_text("Crear perfil").clicked() {
                         self.create_profile_modal_name = Some(String::new());
                    }
                    if tui_button_c(ui, "DL", tui_theme::NEON_YELLOW).on_hover_text("Instalar/Descargar").clicked() {
                        if let Some(selected) = &self.selected_profile_name {
                             self.download_confirmation_name = Some(selected.clone());
                             self.download_source = DownloadSource::Profile(selected.clone());
                        } else {
                            self.status_msg = "Selecciona un perfil primero.".to_string();
                        }   
                    }
                });
                
                tui_separator(ui);

                ScrollArea::vertical().show(ui, |ui| {
                    let mut names: Vec<String> = self.profiles_db.profiles.keys().cloned().collect();
                    names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
                    for name in names {
                        ui.horizontal(|ui| {
                            let is_selected = self.selected_profile_name.as_ref() == Some(&name);
                            let indicator = if is_selected { "> " } else { "  " };
                            let color = if is_selected { tui_theme::ACCENT } else { tui_theme::TEXT_PRIMARY };
                            let text = egui::RichText::new(format!("{}{}", indicator, name))
                                .family(egui::FontFamily::Monospace).color(color);
                            if ui.add(egui::Button::new(text)
                                .fill(egui::Color32::TRANSPARENT)
                                .stroke(egui::Stroke::NONE)
                                .corner_radius(egui::CornerRadius::ZERO)).clicked() {
                                if self.selected_profile_name.as_ref() != Some(&name) {
                                    self.selected_profile_name = Some(name.clone());
                                    self.profile_mods_pending_deletion.clear();
                                }
                            }
                            if tui_button_c(ui, "X", tui_theme::NEON_RED).clicked() {
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
                    tui_dim(ui, "Nombre:");
                    ui.text_edit_singleline(&mut profile.name);
                    
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if tui_button_c(ui, "SAVE", tui_theme::NEON_GREEN).on_hover_text("Guardar cambios").clicked() {
                            should_save = true;
                            self.status_msg = "Perfil guardado.".to_string();
                        }
                        ui.add_space(5.0);
                        if tui_button(ui, "BUSCAR").on_hover_text("Buscar / Añadir Mod").clicked() {
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
                
                tui_separator(ui);
                ui.horizontal(|ui| { tui_dim(ui, "Mods: "); tui_number(ui, &profile.mods.len().to_string()); });
                
                ScrollArea::vertical().id_salt("profile_mods_scroll").show(ui, |ui| {
                    // Collect toggle actions to avoid borrowing issues in loop
                    let mut to_mark = Vec::new();
                    let mut to_unmark = Vec::new();

                    let mut sorted_mod_keys: Vec<String> = profile.mods.keys().cloned().collect();
                    sorted_mod_keys.sort_by(|a, b| {
                        let na = profile.mods.get(a).map(|m| m.name.to_lowercase()).unwrap_or_default();
                        let nb = profile.mods.get(b).map(|m| m.name.to_lowercase()).unwrap_or_default();
                        na.cmp(&nb)
                    });
                    for k in &sorted_mod_keys {
                    let m = &profile.mods[k];
                        let is_pending = self.profile_mods_pending_deletion.contains(k);
                        ui.horizontal(|ui| {
                            if is_pending {
                                ui.label(egui::RichText::new(&m.name)
                                    .family(egui::FontFamily::Monospace)
                                    .strikethrough()
                                    .color(tui_theme::TEXT_DIM));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.add_space(15.0);
                                    if tui_button_c(ui, "UNDO", tui_theme::NEON_GREEN).on_hover_text("Restaurar mod").clicked() {
                                        to_unmark.push(k.clone());
                                    }
                                });
                            } else {
                                ui.label(egui::RichText::new(&m.name)
                                    .family(egui::FontFamily::Monospace)
                                    .color(tui_theme::TEXT_PRIMARY));
                                
                                // Download status is shown in DESCARGA window

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.add_space(15.0);
                                    if tui_button_c(ui, "X", tui_theme::NEON_RED).on_hover_text("Marcar para borrar").clicked() {
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
            tui_dim(ui, "Selecciona un perfil o crea uno nuevo.");
        }
    }

    fn load_all_datapacks(&mut self) {
        self.world_datapacks.clear();
        self.cached_worlds = list_worlds();
        // Pre-create empty entries for each world so they appear immediately
        for w in &self.cached_worlds {
            self.world_datapacks.entry(w.clone()).or_insert_with(IndexMap::new);
        }
        // Enqueue async read jobs for all .zip files in each world
        for world in &self.cached_worlds {
            let dp_folder = PATHS.saves_folder.join(world).join("datapacks");
            if let Ok(entries) = std::fs::read_dir(&dp_folder) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("zip") {
                        let _ = self.tx_dp_read_jobs.send(DatapackReadJob {
                            file_path: path,
                            world_name: world.clone(),
                        });
                    }
                }
            }
        }
        self.datapacks_loaded = true;
        self.status_msg = format!("Escaneando datapacks en {} mundos...", self.cached_worlds.len());
    }

    fn render_datapacks_center(&mut self, ui: &mut egui::Ui) {
        tui_heading(ui, "DATAPACKS");
        ui.add_space(2.0);

        ui.horizontal(|ui| {
            if tui_button(ui, "F5").on_hover_text("Recargar mundos y datapacks").clicked() {
                self.datapacks_loaded = false;
                self.load_all_datapacks();
            }
        });
        ui.add_space(8.0);

        if self.cached_worlds.is_empty() {
            tui_dim(ui, "(no se encontraron mundos en saves/)");
            return;
        }

        ScrollArea::vertical().id_salt("datapacks_scroll").show(ui, |ui| {
            let mut worlds_sorted = self.cached_worlds.clone();
            worlds_sorted.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

            for world in &worlds_sorted {
                let packs = self.world_datapacks.get(world);
                let count = packs.map(|p| p.len()).unwrap_or(0);

                let header_text = format!("{}  ({})", world, count);
                egui::CollapsingHeader::new(
                    egui::RichText::new(&header_text)
                        .family(egui::FontFamily::Monospace)
                        .color(tui_theme::ACCENT)
                        .strong()
                )
                .id_salt(format!("world_{}", world))
                .default_open(false)
                .show(ui, |ui| {
                    if count == 0 {
                        tui_dim(ui, "  (vacío)");
                        return;
                    }

                    let packs_map = self.world_datapacks.get(world).unwrap();
                    let mut sorted_keys: Vec<String> = packs_map.keys().cloned().collect();
                    sorted_keys.sort_by(|a, b| {
                        let na = packs_map.get(a).map(|d| d.name.to_lowercase()).unwrap_or_default();
                        let nb = packs_map.get(b).map(|d| d.name.to_lowercase()).unwrap_or_default();
                        na.cmp(&nb)
                    });

                    for key in &sorted_keys {
                        if let Some(dp) = packs_map.get(key) {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    // Name line
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new(&dp.name)
                                            .family(egui::FontFamily::Monospace)
                                            .color(tui_theme::TEXT_PRIMARY)
                                            .strong());
                                        if let Some(v) = &dp.version_local {
                                            ui.label(egui::RichText::new(format!("v{}", v))
                                                .family(egui::FontFamily::Monospace)
                                                .color(tui_theme::TEXT_DIM)
                                                .size(11.0));
                                        }
                                    });

                                    // Info line: pack_format → MC version
                                    let mut info_parts: Vec<String> = Vec::new();
                                    if let Some(pf) = dp.pack_format {
                                        let mc = dp.mc_version.as_deref().unwrap_or("?");
                                        info_parts.push(format!("pack{} → MC {}", pf, mc));
                                    }
                                    if let Some((min, max)) = dp.supported_formats {
                                        if min != max {
                                            info_parts.push(format!("formatos {}-{}", min, max));
                                        }
                                    }
                                    if let Some(slug) = &dp.detected_project_id {
                                        info_parts.push(format!("slug: {}", slug));
                                    }
                                    if let Some(size) = dp.file_size_bytes {
                                        let size_str = if size >= 1_048_576 {
                                            format!("{:.1} MB", size as f64 / 1_048_576.0)
                                        } else {
                                            format!("{:.0} KB", size as f64 / 1024.0)
                                        };
                                        info_parts.push(size_str);
                                    }
                                    if !info_parts.is_empty() {
                                        tui_dim(ui, &format!("├── {}", info_parts.join("  |  ")));
                                    }
                                });
                            });
                            ui.separator();
                        }
                    }
                });
            }
        });
    }
}

impl eframe::App for ModUpdaterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Top Bar (Tabs) ---
        TopBottomPanel::top("top_tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if tui_tab(ui, "MODS", self.current_tab == AppTab::Explorer).clicked() {
                    self.current_tab = AppTab::Explorer;
                }
                if tui_tab(ui, "PERFILES", self.current_tab == AppTab::Profiles).clicked() {
                    self.current_tab = AppTab::Profiles;
                }
                if tui_tab(ui, "DATAPACKS", self.current_tab == AppTab::Datapacks).clicked() {
                    self.current_tab = AppTab::Datapacks;
                    if !self.datapacks_loaded {
                        self.load_all_datapacks();
                    }
                }
            });
        });

        // --- Status Bar ---
        TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                tui_dim(ui, &format!(">> {}", self.status_msg));
            });
        });

        // --- Side Panels (Must be called before CentralPanel) ---
        match self.current_tab {
            AppTab::Explorer => self.render_explorer_side(ctx),
            AppTab::Profiles => self.render_profiles_side(ctx),
            AppTab::Datapacks => {}, // Sin sidebar
        }

        // --- Main Content ---
        CentralPanel::default().show(ctx, |ui| {
            match self.current_tab {
                AppTab::Explorer => self.render_explorer_center(ui),
                AppTab::Profiles => self.render_profiles_center(ui),
                AppTab::Datapacks => self.render_datapacks_center(ui),
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
                        crate::local_mods_ops::cache::update_remote_info(&key, confirmed_project_id, version_remote);
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

        // --- Datapack Read Events ---
        for ev in self.rx_dp_read_events.try_iter() {
            match ev {
                DatapackReadEvent::Done { world_name, info } => {
                    let key = info.key.clone();
                    self.world_datapacks
                        .entry(world_name)
                        .or_insert_with(IndexMap::new)
                        .insert(key, info);
                }
                DatapackReadEvent::Error { world_name, path, msg } => {
                    println!("Error reading datapack {:?}: {}", path, msg);
                    let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    let fallback = DatapackInfo {
                        key: filename.clone(),
                        name: filename.clone(),
                        selected: true,
                        ..Default::default()
                    };
                    self.world_datapacks
                        .entry(world_name)
                        .or_insert_with(IndexMap::new)
                        .insert(filename, fallback);
                }
            }
        }

        // --- Global Deletion Modal ---
        if self.deletion_confirmation != DeletionConfirmation::None {
            egui::Window::new("CONFIRMAR")
                .collapsible(true)
                .resizable(false)
                .show(ctx, |ui| {
                    match &self.deletion_confirmation {
                        DeletionConfirmation::Modpack(name) => { tui_dim(ui, &format!("Borrar modpack '{}' de disco?", name)); },
                        DeletionConfirmation::SelectedMods => {
                            if let Some(mp) = &self.selected_modpack_ui {
                                if self.active_modpack.as_ref() == Some(mp) {
                                    tui_theme::tui_status(ui, "[!] MODPACK ACTIVO", tui_theme::WARNING);
                                    tui_dim(ui, "Se borraran mods del modpack Y del juego.");
                                } else {
                                    tui_dim(ui, "Borrar mods seleccionados del modpack?");
                                }
                            } else {
                                tui_dim(ui, "Borrar mods seleccionados de disco?");
                            }
                        },
                        DeletionConfirmation::Profile(name) => { tui_dim(ui, &format!("Borrar perfil '{}'? (No borra archivos)", name)); },
                        DeletionConfirmation::None => {},
                    };
                    
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if tui_button_c(ui, "CANCEL", tui_theme::NEON_RED).clicked() {
                            self.deletion_confirmation = DeletionConfirmation::None;
                        }
                        if tui_button_c(ui, "OK", tui_theme::NEON_RED).clicked() {
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
            let mut close_requested = false;

            egui::Window::new("DESCARGA")
                .collapsible(true)
                .resizable(true)
                .open(&mut open)
                .show(ctx, |ui| {
                    tui_dim(ui, "Carpeta del Modpack:");
                    ui.text_edit_singleline(&mut name);
                    
                    ui.add_space(8.0);
                    tui_separator(ui);
                    ui.add_space(5.0);
                    
                    // 1. Selector de Loader
                    ui.horizontal(|ui| {
                        tui_dim(ui, "Loader:");
                        egui::ComboBox::from_id_salt("loader-selector-modal")
                            .selected_text(&self.selected_loader)
                            .show_ui(ui, |ui| {
                                for loader in &self.loaders {
                                    ui.selectable_value(&mut self.selected_loader, loader.clone(), loader);
                                }
                            });

                        // 2. Selector de Versión MC
                        tui_dim(ui, "Version:");
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
                        if tui_button_c(ui, "CANCEL", tui_theme::NEON_RED).clicked() {
                            close_requested = true;
                        }
                        let has_active = self.active_downloads.values().any(|s| matches!(s, ModStatus::Resolving | ModStatus::Downloading(_)));
                        if has_active {
                            tui_dim(ui, "[WAIT]");
                        } else if tui_button_c(ui, "OK", tui_theme::NEON_GREEN).clicked() {
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
                                // Don't close — keep window open to show progress
                            }
                        }
                    });

                    // === Download Progress List ===
                    if !self.active_downloads.is_empty() {
                        ui.add_space(8.0);
                        tui_separator(ui);
                        ui.add_space(4.0);
                        tui_heading(ui, "PROGRESO");
                        ui.add_space(4.0);

                        let mut done_count = 0usize;
                        let mut err_count = 0usize;
                        let total = self.active_downloads.len();

                        egui::ScrollArea::vertical()
                            .id_salt("download_progress_scroll")
                            .max_height(300.0)
                            .show(ui, |ui| {
                                for (key, status) in &self.active_downloads {
                                    ui.horizontal(|ui| {
                                        let display_name = key.as_str();

                                        match status {
                                            ModStatus::Resolving => {
                                                tui_theme::tui_status(ui, "[...]", tui_theme::NEON_YELLOW);
                                                tui_dim(ui, &display_name);
                                            },
                                            ModStatus::Downloading(p) => {
                                                let pct = format!("[{:>3.0}%]", p * 100.0);
                                                tui_theme::tui_status(ui, &pct, tui_theme::NEON_YELLOW);
                                                tui_dim(ui, &display_name);
                                            },
                                            ModStatus::Done => {
                                                done_count += 1;
                                                tui_theme::tui_status(ui, "[ OK ]", tui_theme::NEON_GREEN);
                                                tui_dim(ui, &display_name);
                                            },
                                            ModStatus::Error(e) => {
                                                err_count += 1;
                                                tui_theme::tui_status(ui, "[FAIL]", tui_theme::NEON_RED);
                                                tui_dim(ui, &display_name);
                                                ui.label(egui::RichText::new(e)
                                                    .family(egui::FontFamily::Monospace)
                                                    .color(tui_theme::NEON_RED)
                                                    .size(10.0));
                                            },
                                            _ => {
                                                tui_dim(ui, &display_name);
                                            }
                                        }
                                    });
                                }
                            });

                        ui.add_space(4.0);
                        tui_separator(ui);
                        ui.horizontal(|ui| {
                            tui_number(ui, &format!("{}/{}", done_count, total));
                            tui_dim(ui, "completados");
                            if err_count > 0 {
                                tui_theme::tui_status(ui, &format!("{} errores", err_count), tui_theme::NEON_RED);
                            }
                        });

                        // Clear button when all done
                        if done_count + err_count == total {
                            ui.add_space(4.0);
                            if tui_button_c(ui, "LIMPIAR", tui_theme::NEON_GREEN).clicked() {
                                self.active_downloads.clear();
                            }
                        }
                    }
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

            egui::Window::new("CREAR PERFIL")
                .collapsible(true)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    tui_dim(ui, "Nombre del Perfil:");
                    ui.text_edit_singleline(&mut name);
                    ui.add_space(8.0);
                    
                    ui.horizontal(|ui| {
                        if tui_button_c(ui, "CANCEL", tui_theme::NEON_RED).clicked() {
                            close_requested = true;
                        }
                        if tui_button_c(ui, "SAVE", tui_theme::NEON_GREEN).clicked() {
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
        while let Ok((mut new_results, _source, offset)) = self.rx_search.try_recv() {
            // Fire off dep-name resolution for each result that has a project id
            let cf_key = crate::fetch::cf_api_key();
            let version = self.search_state.version.clone();
            let loader = self.search_state.loader.clone();
            for res in &mut new_results {
                let project_id = res.modrinth_id.clone()
                    .or_else(|| res.curseforge_id.map(|id| id.to_string()));
                if let Some(pid) = project_id {
                    res.fetching_dependencies = true;
                    let _ = self.tx_fetch_dep_names.send((
                        res.slug.clone(),
                        pid,
                        version.clone(),
                        loader.clone(),
                        cf_key.clone(),
                    ));
                }
            }
            if offset == 0 {
                self.search_state.results = new_results;
            } else {
                self.search_state.results.extend(new_results);
            }
            self.search_state.is_searching = false;
        }

        // --- Dependency Resolution Results Poll ---
        // When the resolver thread finishes, update the matching search card with dep names
        while let Ok((mod_key, dep_names)) = self.rx_dep_resolved.try_recv() {
            if let Some(result) = self.search_state.results.iter_mut().find(|r| r.name == mod_key) {
                result.dependencies = Some(dep_names);
            }
        }

        // --- Dep-Name Preview Results Poll ---
        while let Ok((slug, dep_names)) = self.rx_dep_names_result.try_recv() {
            if let Some(result) = self.search_state.results.iter_mut().find(|r| r.slug == slug) {
                result.dependencies = Some(dep_names);
                result.fetching_dependencies = false;
            }
        }

        // --- Profile Dependency Resolution Results Poll ---
        while let Ok((profile_name, deps)) = self.rx_profile_deps_resolved.try_recv() {
            if let Some(profile) = self.profiles_db.get_profile_mut(&profile_name) {
                for (dep_name, dep_filename, dep_project_id, dep_slug) in deps {
                    if !profile.contains_mod(&dep_filename, &dep_project_id, &dep_slug) {
                        profile.mods.insert(
                            dep_filename.clone(),
                            crate::local_mods_ops::ModInfo::from_dep(dep_filename, dep_name, dep_project_id, dep_slug),
                        );
                    }
                }
                save_profiles(&self.profiles_db);
                self.status_msg = format!("Dependencias añadidas al perfil '{}'.", profile_name);
            }
        }
    }
}

impl ModUpdaterApp {
    fn render_search_modal(&mut self, ctx: &egui::Context) {
        let mut open = self.search_state.open;
        if !open { return; }

        let title = if let SearchSource::Profile(p) = &self.search_state.source {
            format!("BUSCAR para '{}'", p)
        } else {
            "BUSCAR MODS".to_string()
        };

        egui::Window::new(&title)
            .open(&mut open)
            .resize(|r| r.fixed_size(egui::vec2(700.0, 600.0))) // Start larger
            .show(ctx, |ui| {
                // Filters Row (Only for Explorer / Direct Download)
                let is_explorer = matches!(self.search_state.source, SearchSource::Explorer);
                
                if is_explorer {
                    ui.horizontal(|ui| {
                        tui_dim(ui, "Loader:");
                        egui::ComboBox::from_id_salt("search_loader")
                            .selected_text(&self.search_state.loader)
                            .show_ui(ui, |ui| {
                                for l in &self.loaders {
                                    ui.selectable_value(&mut self.search_state.loader, l.clone(), l);
                                }
                            });

                        tui_dim(ui, "Version:");
                        egui::ComboBox::from_id_salt("search_version_selector")
                            .selected_text(&self.search_state.version)
                            .show_ui(ui, |ui| {
                                for v in &self.mc_versions {
                                    ui.selectable_value(&mut self.search_state.version, v.clone(), v);
                                }
                            });
                            
                    });
                    ui.add_space(5.0);

                    // --- Version/Loader mismatch warning ---
                    // Only shown when a modpack is selected and its mods are loaded
                    if !self.mods.is_empty() {
                        let loader_keys = ["fabricloader", "fabric-loader", "forge", "neoforge", "quilt_loader"];

                        // Detect dominant MC version from depends["minecraft"] of loaded mods
                        let mut ver_count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
                        for m in self.mods.values() {
                            if let Some(deps) = &m.depends {
                                if let Some(mc_ver) = deps.get("minecraft") {
                                    let clean = format_version_range(mc_ver);
                                    // Take only the first part if it's a range (e.g. "1.21.10 - 1.21.11" → "1.21.10")
                                    let base = clean.split_whitespace().next().unwrap_or(&clean).to_string();
                                    if !base.is_empty() && base != "*" {
                                        *ver_count.entry(base).or_insert(0) += 1;
                                    }
                                }
                            }
                        }
                        let modpack_version = ver_count.into_iter().max_by_key(|(_, c)| *c).map(|(v, _)| v);

                        // Detect dominant loader from depends keys
                        let modpack_loader = self.mods.values().find_map(|m| {
                            m.depends.as_ref()?.keys()
                                .find(|k| loader_keys.contains(&k.as_str()))
                                .map(|k| format_dep_name(k))
                        });

                        let search_ver = &self.search_state.version;
                        let search_loader = &self.search_state.loader;

                        let ver_mismatch = modpack_version.as_ref().map_or(false, |v| v != search_ver);
                        let loader_mismatch = modpack_loader.as_ref().map_or(false, |l| {
                            l.to_lowercase() != search_loader.to_lowercase()
                        });

                        if ver_mismatch || loader_mismatch {
                            ui.add_space(2.0);
                            if ver_mismatch {
                                let msg = format!(
                                    "⚠  Versión del modpack: {}  —  buscando para: {}",
                                    modpack_version.as_deref().unwrap_or("?"),
                                    search_ver
                                );
                                tui_theme::tui_status(ui, &msg, tui_theme::WARNING);
                            }
                            if loader_mismatch {
                                let msg = format!(
                                    "⚠  Loader del modpack: {}  —  buscando para: {}",
                                    modpack_loader.as_deref().unwrap_or("?"),
                                    search_loader
                                );
                                tui_theme::tui_status(ui, &msg, tui_theme::WARNING);
                            }
                            ui.add_space(2.0);
                        }
                    }
                }

                ui.horizontal(|ui| {
                    super::tui_theme::tui_checkbox(ui, &mut self.search_state.download_dependencies);
                    tui_dim(ui, "Añadir dependencias");
                });
                ui.add_space(3.0);

                ui.horizontal(|ui| {
                    tui_dim(ui, "Buscar:");
                    let text_box = ui.text_edit_singleline(&mut self.search_state.query);
                    
                    let mut do_search = false;

                    if text_box.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                         do_search = true;
                    }
                    if tui_button(ui, "GO").clicked() {
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
                    tui_theme::tui_status(ui, "[...] Buscando...", tui_theme::TEXT_DIM);
                }

                tui_separator(ui);

                egui::ScrollArea::vertical().max_height(450.0).show(ui, |ui| {
                    for res in &self.search_state.results {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new(&res.name)
                                        .family(egui::FontFamily::Monospace)
                                        .color(tui_theme::TEXT_PRIMARY).strong());
                                    tui_dim(ui, &res.author);
                                    tui_dim(ui, &res.description);
                                    
                                    // Source badges
                                    ui.horizontal(|ui| {
                                        if res.modrinth_id.is_some() { tui_theme::tui_status(ui, "[MR]", tui_theme::NEON_GREEN); }
                                        if res.curseforge_id.is_some() { tui_theme::tui_status(ui, "[CF]", tui_theme::NEON_YELLOW); }
                                        if res.fetching_dependencies { ui.spinner(); tui_dim(ui, "deps..."); }
                                    });

                                    // Dependencies collapsible list
                                    if let Some(deps) = &res.dependencies {
                                        if !deps.is_empty() {
                                            let header_text = format!("Dependencias ({})", deps.len());
                                            egui::CollapsingHeader::new(
                                                egui::RichText::new(&header_text)
                                                    .family(egui::FontFamily::Monospace)
                                                    .color(tui_theme::TEXT_DIM)
                                                    .size(11.0)
                                            )
                                            .id_salt(format!("deps_{}", &res.slug))
                                            .default_open(false)
                                            .show(ui, |ui| {
                                                for dep in deps {
                                                    ui.horizontal(|ui| {
                                                        tui_dim(ui, "•");
                                                        ui.label(
                                                            egui::RichText::new(dep)
                                                                .family(egui::FontFamily::Monospace)
                                                                .color(tui_theme::TEXT_DIM)
                                                                .size(11.0)
                                                        );
                                                    });
                                                }
                                            });
                                        } else if !res.fetching_dependencies {
                                            tui_dim(ui, "├── Sin dependencias");
                                        }
                                    }
                                });

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    // Contextual Action
                                    match &self.search_state.source {
                                        SearchSource::Explorer => {
                                            // Check progress by name
                                            if let Some(status) = self.active_downloads.get(&res.name) {
                                                match status {
                                                    ModStatus::Done => { tui_theme::tui_status(ui, "[OK]", tui_theme::NEON_GREEN); },
                                                    _ => { tui_theme::tui_status(ui, "[...]", tui_theme::NEON_YELLOW); },
                                                }
                                            } else {
                                                if tui_button_c(ui, "DL", tui_theme::NEON_YELLOW).clicked() {
                                                    // Trigger Download
                                                    // Construct ModInfo
                                                    let mod_info = crate::local_mods_ops::ModInfo {
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
                                                        crate::local_mods_ops::prepare_output_folder(&self.selected_mc_version);
                                                        PATHS.modpacks_folder.join(format!(r"mods{}", self.selected_mc_version))
                                                    };
                                                    let _ = std::fs::create_dir_all(&output_folder_path);

                                                         if self.search_state.download_dependencies {
                                                        let mod_id_str = res.modrinth_id.clone()
                                                            .or_else(|| res.curseforge_id.map(|id| id.to_string()))
                                                            .unwrap_or_else(|| res.slug.clone());

                                                        // Build existing sets from self.mods (selected modpack in-memory — source of truth)
                                                        let mut existing_project_ids = std::collections::HashSet::new();
                                                        let mut existing_filenames = std::collections::HashSet::new();
                                                        for (key, m) in &self.mods {
                                                            existing_filenames.insert(key.clone());
                                                            if let Some(id) = &m.confirmed_project_id { existing_project_ids.insert(id.clone()); }
                                                            if let Some(id) = &m.detected_project_id { existing_project_ids.insert(id.clone()); }
                                                        }

                                                        let _ = self.tx_resolve_deps.send((
                                                            res.name.clone(),
                                                            mod_id_str,
                                                            self.search_state.version.clone(),
                                                            self.search_state.loader.clone(),
                                                            output_folder_path.to_string_lossy().to_string(),
                                                            existing_project_ids,
                                                            existing_filenames,
                                                        ));
                                                    }
                                                    
                                                    // ALWAYS download the searched mod
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
                                            // Check if mod is already in the profile
                                            let res_id = res.modrinth_id.clone()
                                                .or_else(|| res.curseforge_id.map(|id| id.to_string()))
                                                .unwrap_or_default();
                                            let already_in_profile = self.profiles_db.get_profile(p_name)
                                                .map_or(false, |p| p.contains_mod(&res.name, &res_id, &res.slug));

                                            if already_in_profile {
                                                tui_theme::tui_status(ui, "[OK]", tui_theme::NEON_GREEN);
                                            } else if tui_button_c(ui, "ADD", tui_theme::NEON_GREEN).clicked() {
                                                let project_id = res.modrinth_id.clone()
                                                    .or_else(|| res.curseforge_id.map(|id| id.to_string()));
                                                if let Some(profile) = self.profiles_db.get_profile_mut(p_name) {
                                                    let mod_info = crate::local_mods_ops::ModInfo::from_search(
                                                        res.name.clone(), project_id.clone(),
                                                    );
                                                    profile.mods.insert(res.name.clone(), mod_info);
                                                }
                                                save_profiles(&self.profiles_db);
                                                self.status_msg = format!("Mod '{}' añadido al perfil.", res.name);

                                                // Auto-add dependencies if checkbox is active
                                                if self.search_state.download_dependencies {
                                                    let mod_id = res.modrinth_id.clone()
                                                        .or_else(|| res.curseforge_id.map(|id| id.to_string()))
                                                        .unwrap_or_else(|| res.slug.clone());

                                                    // Build set of project_ids already in profile
                                                    let existing_ids: std::collections::HashSet<String> = {
                                                        if let Some(profile) = self.profiles_db.get_profile(p_name) {
                                                            profile.mods.values().flat_map(|m| {
                                                                m.confirmed_project_id.iter().chain(m.detected_project_id.iter()).cloned()
                                                            }).collect()
                                                        } else { std::collections::HashSet::new() }
                                                    };

                                                    let _ = self.tx_resolve_profile_deps.send((
                                                        mod_id,
                                                        p_name.clone(),
                                                        self.search_state.version.clone(),
                                                        self.search_state.loader.clone(),
                                                        existing_ids,
                                                    ));
                                                }
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
                            if tui_button(ui, "MAS").clicked() {
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
