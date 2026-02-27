use eframe::egui;

use crate::paths_vars::PATHS;
use crate::fetch::single_mod_search::SearchRequest;
use crate::fetch::async_download::DownloadJob;
use crate::profiles::save_profiles;

use super::utils::{format_version_range, format_dep_name};
use super::tui_theme::{self, tui_button, tui_button_c, tui_separator, tui_dim};
use super::types::{SearchSource, ModStatus};

impl super::app::ModUpdaterApp {
    pub(crate) fn render_search_modal(&mut self, ctx: &egui::Context) {
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
