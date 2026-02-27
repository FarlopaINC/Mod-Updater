use eframe::{egui, egui::CentralPanel};
use std::collections::{HashSet, HashMap};
use eframe::egui::TopBottomPanel;
use crossbeam_channel::{unbounded, Sender, Receiver};
use std::thread;
use indexmap::IndexMap;

use crate::local_mods_ops::{
    get_minecraft_versions,
    spawn_read_workers, ReadJob, ReadEvent,
};
use crate::local_datapacks_ops::{
    DatapackInfo, DatapackReadJob, DatapackReadEvent,
    spawn_datapack_read_workers,
};
use crate::profiles::{ProfilesDatabase, load_profiles};
use crate::fetch::async_download::{spawn_workers, DownloadJob, DownloadEvent};
use crate::fetch::single_mod_search::{UnifiedSearchResult, search_unified, SearchRequest};
use crate::paths_vars::PATHS;
use super::tui_theme::{self, tui_tab, tui_dim};

// Import our newly extracted UI modules and types
pub(crate) use super::types::{
    ModStatus, UiModInfo, DeletionConfirmation, DownloadSource, AppTab, SearchSource, SearchState
};

pub struct ModUpdaterApp {
    // --- Shared State ---
    pub(crate) mc_versions: Vec<String>,
    pub(crate) selected_mc_version: String,
    pub(crate) status_msg: String,
    pub(crate) current_tab: AppTab,

    // --- Explorer State ---
    pub(crate) mods: IndexMap<String, UiModInfo>,
    pub(crate) tx_jobs: Sender<DownloadJob>,
    pub(crate) rx_events: Receiver<DownloadEvent>,
    
    // --- Async Read State ---
    pub(crate) tx_read_jobs: Sender<ReadJob>,
    pub(crate) rx_read_events: Receiver<ReadEvent>,

    pub(crate) deletion_confirmation: DeletionConfirmation,

    // --- Profiles State ---
    pub(crate) profiles_db: ProfilesDatabase,
    pub(crate) selected_profile_name: Option<String>,
    pub(crate) profile_mods_pending_deletion: HashSet<String>,


    // --- UI Selection State ---
    pub(crate) loaders: Vec<String>,
    pub(crate) selected_loader: String,
    pub(crate) selected_modpack_ui: Option<String>,
    
    // --- Download Dialog State ---
    pub(crate) download_confirmation_name: Option<String>,
    pub(crate) download_source: DownloadSource,
    
    // --- Create Profile Dialog State ---
    pub(crate) create_profile_modal_name: Option<String>,
    
    // --- Modpacks State ---
    pub(crate) active_modpack: Option<String>,
    pub(crate) cached_modpacks: Vec<String>,
    
    // --- Global Download State ---
    pub(crate) active_downloads: HashMap<String, ModStatus>,

    // --- Search State ---
    pub(crate) search_state: SearchState,
    pub(crate) tx_search: Sender<(SearchRequest, SearchSource)>, // Request, Source
    pub(crate) rx_search: Receiver<(Vec<UnifiedSearchResult>, SearchSource, u32)>, // Results, Source, Offset

    // --- Dependency Resolution State ---
    // (mod_key, project_id, version, loader, output_folder, existing_project_ids, existing_filenames)
    pub(crate) tx_resolve_deps: Sender<(String, String, String, String, String, std::collections::HashSet<String>, std::collections::HashSet<String>)>,
    // Returns (mod_key, list_of_dep_display_names) so the search card can update
    pub(crate) rx_dep_resolved: Receiver<(String, Vec<String>)>,

    // --- Profile Dependency Resolution ---
    // (project_id, profile_name, version, loader, existing_project_ids)
    pub(crate) tx_resolve_profile_deps: Sender<(String, String, String, String, std::collections::HashSet<String>)>,
    // Returns (profile_name, Vec<(dep_name, dep_filename, dep_project_id, dep_slug)>)
    pub(crate) rx_profile_deps_resolved: Receiver<(String, Vec<(String, String, String, String)>)>,

    // --- Search Dep-Name Preview Resolution ---
    // (slug, project_id, version, loader, cf_key)
    pub(crate) tx_fetch_dep_names: Sender<(String, String, String, String, String)>,
    // Returns (slug, dep_names)
    pub(crate) rx_dep_names_result: Receiver<(String, Vec<String>)>,

    // --- Datapacks State ---
    pub(crate) cached_worlds: Vec<String>,
    pub(crate) world_datapacks: IndexMap<String, IndexMap<String, DatapackInfo>>,
    pub(crate) tx_dp_read_jobs: Sender<DatapackReadJob>,
    pub(crate) rx_dp_read_events: Receiver<DatapackReadEvent>,
    pub(crate) datapacks_loaded: bool,
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

            cached_modpacks: crate::local_mods_ops::list_modpacks(),

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
        self.process_download_events();
        self.process_read_events();
        self.process_datapack_events();

        // --- Modals ---
        self.render_deletion_modal(ctx);
        self.render_download_modal(ctx);
        self.render_create_profile_modal(ctx);
        self.render_search_modal(ctx);

        // --- Background Search / Dependency Events ---
        self.process_search_events();
    }
}
