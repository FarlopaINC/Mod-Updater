use eframe::egui;

use crate::paths_vars::PATHS;
use crate::fetch::search_provider::{SearchRequest, ContentType};
use crate::fetch::async_download::DownloadJob;


use super::utils::{format_version_range, format_dep_name};
use super::tui_theme::{self, tui_button, tui_button_c, tui_separator, tui_dim};
use super::types::{SearchSource, ModStatus};

impl super::app::ModUpdaterApp {
    pub(crate) fn render_search_center(&mut self, ui: &mut egui::Ui) {
        if !self.search_state.open { return; }

        let title = match &self.search_state.source {
            SearchSource::Profile(p) => format!("BUSCAR para '{}'", p),
            SearchSource::World(w) => format!("BUSCAR DATAPACKS — {}", w),
            _ => format!("BUSCAR {}", self.search_state.content_type.display_name().to_uppercase()),
        };

        ui.horizontal(|ui| {
            if tui_button_c(ui, "<- ATRÁS", tui_theme::NEON_RED).clicked() {
                self.search_state.open = false;
            }
            ui.add_space(8.0);
            tui_theme::tui_heading(ui, &title);
        });
        ui.add_space(8.0);

        // Filters Row (Only for Explorer / World / Direct Download)
                let is_explorer = matches!(self.search_state.source, SearchSource::Explorer | SearchSource::World(_));
                let supports_loader = self.search_state.content_type == ContentType::Mod;
                
                if is_explorer {
                    ui.horizontal(|ui| {
                        if supports_loader {
                            tui_dim(ui, "Loader:");
                            egui::ComboBox::from_id_salt("search_loader")
                                .selected_text(&self.search_state.loader)
                                .show_ui(ui, |ui| {
                                    for l in &self.loaders {
                                        ui.selectable_value(&mut self.search_state.loader, l.clone(), l);
                                    }
                                });
                        }

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
                                (
                                    if supports_loader { Some(self.search_state.loader.clone()) } else { None },
                                    Some(self.search_state.version.clone()),
                                )
                            } else {
                                (None, None)
                            };

                            let req = SearchRequest {
                                query: self.search_state.query.clone(),
                                loader,
                                version,
                                offset: 0,
                                limit: self.search_state.limit,
                                content_type: self.search_state.content_type,
                            };
                            let _ = self.tx_search.send((req, self.search_state.source.clone()));
                        }
                    }
                });
                
                if self.search_state.is_searching && self.search_state.page == 0 {
                    tui_theme::tui_status(ui, "[...] Buscando...", tui_theme::TEXT_DIM);
                }

                tui_separator(ui);

                let selected_project_opt = self.search_state.selected_project_for_versions.clone();
                if let Some(selected_project) = selected_project_opt {
                    // --- VERSION SELECTION VIEW ---
                    ui.horizontal(|ui| {
                        if tui_button(ui, "<- VOLVER").clicked() {
                            self.search_state.selected_project_for_versions = None;
                            self.search_state.project_versions_results.clear();
                        }
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new(format!("Versiones de: {}", selected_project.name))
                            .family(egui::FontFamily::Monospace)
                            .color(tui_theme::TEXT_PRIMARY).strong());
                    });
                    
                    tui_separator(ui);

                    if self.search_state.is_fetching_versions {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            tui_dim(ui, " Obteniendo versiones...");
                        });
                    } else if self.search_state.project_versions_results.is_empty() {
                        tui_theme::tui_status(ui, "No se encontraron versiones compatibles.", tui_theme::WARNING);
                    } else {
                        egui::ScrollArea::vertical().max_height(450.0).show(ui, |ui| {
                            for ver in &self.search_state.project_versions_results {
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        // Left side: Version Info
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                // Release type badge (R/A/B)
                                                let (rl_text, rl_color) = match ver.release_type.as_str() {
                                                    "R" => ("[R]", tui_theme::NEON_GREEN),
                                                    "B" => ("[B]", tui_theme::NEON_YELLOW),
                                                    "A" => ("[A]", tui_theme::NEON_RED),
                                                    _ => ("[?]", tui_theme::TEXT_DIM),
                                                };
                                                tui_theme::tui_status(ui, rl_text, rl_color);
                                                
                                                ui.label(egui::RichText::new(&ver.version_number)
                                                    .family(egui::FontFamily::Monospace)
                                                    .color(tui_theme::TEXT_PRIMARY).strong()
                                                );
                                                
                                                tui_dim(ui, &format!("({})", ver.date_published));
                                            });
                                            
                                            // Game versions
                                            if !ver.game_versions.is_empty() {
                                                tui_dim(ui, &format!("MC: {}", ver.game_versions.join(", ")));
                                            }
                                        });

                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            match &self.search_state.source {
                                                SearchSource::Explorer | SearchSource::World(_) => {
                                                    // In Explorer, we show DL button for this specific version
                                                    if let Some(status) = self.active_downloads.get(&ver.version_number) { // Using version_number as key temporarily to avoid collision
                                                        match status {
                                                            ModStatus::Done => { tui_theme::tui_status(ui, "[OK]", tui_theme::NEON_GREEN); },
                                                            _ => { tui_theme::tui_status(ui, "[...]", tui_theme::NEON_YELLOW); },
                                                        }
                                                    } else {
                                                        if tui_button_c(ui, "DL", tui_theme::NEON_YELLOW).clicked() {
                                                            // Trigger Download FOR THIS VERSION
                                                            let mod_info = crate::local_mods_ops::ModInfo {
                                                                key: ver.version_number.clone(), // Important: Use filename/version string to be unique
                                                                name: selected_project.name.clone(),
                                                                detected_project_id: selected_project.modrinth_id.clone().or_else(|| Some(selected_project.slug.clone())),
                                                                confirmed_project_id: selected_project.modrinth_id.clone().or_else(|| selected_project.curseforge_id.map(|id| id.to_string())),
                                                                version_local: Some("".to_string()),
                                                                version_remote: None,
                                                                selected: true,
                                                                file_size_bytes: None,
                                                                file_mtime_secs: None,
                                                                depends: None,
                                                                has_local_icon: false,
                                                            };
                                                            
                                                            let output_folder_path = match &self.search_state.source {
                                                                SearchSource::World(world_name) => PATHS.saves_folder.join(world_name).join("datapacks"),
                                                                _ => if let Some(mp) = &self.selected_modpack_ui {
                                                                    PATHS.modpacks_folder.join(mp)
                                                                } else {
                                                                    crate::local_mods_ops::prepare_output_folder(&self.selected_mc_version);
                                                                    PATHS.modpacks_folder.join(format!(r"mods{}", self.selected_mc_version))
                                                                },
                                                            };
                                                            let _ = std::fs::create_dir_all(&output_folder_path);

                                                            // Handle dependencies
                                                            let mut existing_project_ids = std::collections::HashSet::new();
                                                            if self.search_state.download_dependencies {
                                                                for m in self.mods.values() {
                                                                    if let Some(id) = &m.confirmed_project_id { existing_project_ids.insert(id.clone()); }
                                                                    if let Some(id) = &m.detected_project_id { existing_project_ids.insert(id.clone()); }
                                                                }
                                                            }
                                                            
                                                            let job = DownloadJob {
                                                                key: ver.version_number.clone(),
                                                                modinfo: mod_info.clone(),
                                                                output_folder: output_folder_path.to_string_lossy().to_string(),
                                                                selected_version: ver.id.clone(), // EXACT VERSION ID
                                                                selected_loader: self.search_state.loader.clone(),
                                                                content_type: self.search_state.content_type,
                                                                replaces_filename: None,
                                                                raw_game_version: self.search_state.version.clone(),
                                                                pre_resolved: None,
                                                            };
                                                            
                                                            let _ = self.tx_prepare_downloads.send((
                                                                job,
                                                                self.search_state.download_dependencies,
                                                                existing_project_ids,
                                                            ));
                                                        }
                                                    }
                                                },
                                                SearchSource::Profile(_p_name) => {
                                                    if tui_button_c(ui, "ADD", tui_theme::NEON_GREEN).clicked() {
                                                         // Future Profile version selection implementation
                                                    }
                                                }
                                            }
                                        });
                                    });
                                });
                            }
                        });
                    }

                } else {
                    // --- SEARCH RESULTS VIEW (Existing code) ---
                    egui::ScrollArea::vertical().max_height(450.0).show(ui, |ui| {
                        for res in &self.search_state.results {
                            let shape_idx = ui.painter().add(egui::Shape::Noop);
                            let group_res = ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    // 1. Image on the left
                                    if let Some(icon_url) = &res.icon_url {
                                        let img = egui::Image::new(icon_url)
                                            .fit_to_exact_size(egui::vec2(48.0, 48.0))
                                            .corner_radius(4.0);
                                        ui.add(img);
                                    } else {
                                        ui.allocate_space(egui::vec2(48.0, 48.0));
                                    }

                                    ui.add_space(8.0); // Spacing between image and text
                                    
                                    // 2. Middle section: text (occupies remaining width and aligns left)
                                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                        ui.set_width(ui.available_width());
                                        
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new(&res.name)
                                                .family(egui::FontFamily::Monospace)
                                                .color(tui_theme::TEXT_PRIMARY).strong());
                                            
                                            if res.modrinth_id.is_some() { tui_theme::tui_status(ui, "[MR]", tui_theme::NEON_GREEN); }
                                            if res.curseforge_id.is_some() { tui_theme::tui_status(ui, "[CF]", tui_theme::NEON_YELLOW); }
                                        });
                                        
                                        tui_dim(ui, &res.author);
                                        
                                        ui.add(egui::Label::new(
                                            egui::RichText::new(&res.description)
                                                .family(egui::FontFamily::Monospace)
                                                .color(tui_theme::TEXT_DIM)
                                                .size(11.0)
                                        ).truncate());
                                    });
                                });
                            });

                            let interact = ui.interact(group_res.response.rect, ui.id().with(&res.slug), egui::Sense::click());
                            
                            if interact.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                ui.painter().set(
                                    shape_idx, 
                                    egui::Shape::rect_filled(group_res.response.rect, 0.0, tui_theme::BG_HOVER)
                                );
                            }

                            if interact.clicked() {
                                self.search_state.selected_project_for_versions = Some(res.clone());
                                self.search_state.is_fetching_versions = true;
                                self.search_state.project_versions_results.clear();

                                let project_id = res.modrinth_id.clone()
                                    .or_else(|| res.curseforge_id.map(|id| id.to_string()))
                                    .unwrap_or_else(|| res.slug.clone());

                                let _ = self.tx_fetch_versions.send((
                                    project_id,
                                    self.search_state.loader.clone(),
                                    self.search_state.version.clone(),
                                    self.search_state.content_type,
                                ));
                            }
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
                                    
                                    let is_explorer = matches!(self.search_state.source, SearchSource::Explorer | SearchSource::World(_));
                                    let supports_loader = self.search_state.content_type == ContentType::Mod;
                                    let (loader, version) = if is_explorer {
                                        (
                                            if supports_loader { Some(self.search_state.loader.clone()) } else { None },
                                            Some(self.search_state.version.clone()),
                                        )
                                    } else {
                                        (None, None)
                                    };

                                    let req = SearchRequest {
                                        query: self.search_state.query.clone(),
                                        loader,
                                        version,
                                        offset,
                                        limit: self.search_state.limit,
                                        content_type: self.search_state.content_type,
                                    };
                                    let _ = self.tx_search.send((req, self.search_state.source.clone()));
                                }
                            }
                        }
                    });
                }
    }
}
