use eframe::egui::{self, ScrollArea, SidePanel};

use crate::local_mods_ops::{
    change_mods, 
    read_active_marker,
    ReadJob,
    ModInfo,
};
use crate::paths_vars::PATHS;
use super::utils::{format_dep_name, format_version_range};
use super::tui_theme::{self, tui_button, tui_button_c, tui_checkbox, tui_heading, tui_dim};
use super::types::{DeletionConfirmation, UiModInfo, ModStatus, SearchSource};

impl super::app::ModUpdaterApp {
    /// Carga mods de una carpeta: intenta caché primero, si no, crea placeholder y envía ReadJob.
    pub(crate) fn load_mods_from_folder(&mut self, folder: &std::path::Path) {
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

    pub(crate) fn render_explorer_side(&mut self, ctx: &egui::Context) {
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
                                    let avail = ui.available_size();
                                    if ui.add(egui::Button::new(text)
                                        .fill(egui::Color32::TRANSPARENT)
                                        .stroke(egui::Stroke::NONE)
                                        .corner_radius(egui::CornerRadius::ZERO)
                                        .min_size(avail)).clicked() {
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

    pub(crate) fn render_explorer_center(&mut self, ui: &mut egui::Ui) {
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
}
