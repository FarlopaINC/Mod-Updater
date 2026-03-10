use crate::local_mods_ops::ReadEvent;
use crate::local_datapacks_ops::{DatapackReadEvent, DatapackInfo};
use crate::fetch::async_download::DownloadEvent;
use crate::profiles::save_profiles;
use crate::ui::types::{ModStatus, UiModInfo, DownloadAction, DuplicateResolution};
use indexmap::IndexMap;

impl super::app::ModUpdaterApp {
    pub(crate) fn process_download_events(&mut self) {
        for ev in self.rx_events.try_iter() {
            match ev {
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
    }

    pub(crate) fn process_read_events(&mut self) {
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
    }

    pub(crate) fn process_datapack_events(&mut self) {
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
    }

    pub(crate) fn process_search_events(&mut self) {
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

        // --- Phase 2: Handle Dependency Processing Results ---
        // Converts the returned list of DownloadJobs into DuplicateResolutions for the popup modal
        while let Ok(jobs) = self.rx_prepare_downloads_result.try_recv() {
            let mut resolutions = Vec::new();
            for job in jobs {
                let pid1 = job.modinfo.confirmed_project_id.clone();
                let pid2 = job.modinfo.detected_project_id.clone();
                
                let mut existing_filename = None;
                let mut existing_version = None;
                let mut action = DownloadAction::Install;
                
                // Compare with local mods to find duplicates
                if pid1.is_some() || pid2.is_some() {
                    if let Some((_, m)) = self.mods.iter().find(|(_, m)| {
                        (pid1.is_some() && m.confirmed_project_id == pid1) ||
                        (pid2.is_some() && m.detected_project_id == pid2)
                    }) {
                        existing_filename = Some(m.key.clone());
                        existing_version = m.version_local.clone();
                        action = DownloadAction::Skip; // By default skip if duplicate
                    }
                }
                
                resolutions.push(DuplicateResolution {
                    modinfo: job.modinfo.clone(),
                    download_job: job,
                    existing_filename,
                    existing_version,
                    action,
                    status: ModStatus::Idle,
                });
            }
            // Triggers the modal open because pending_downloads is now Some(_)
            self.pending_downloads = Some(resolutions);
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

        // --- Fetch Versions Results Poll ---
        while let Ok(versions) = self.rx_versions_result.try_recv() {
            self.search_state.project_versions_results = versions;
            self.search_state.is_fetching_versions = false;
        }
    }
}
