use eframe::egui;
use indexmap::IndexMap;

use crate::local_mods_ops::{list_modpacks, ModInfo};
use crate::profiles::{Profile, save_profiles};
use crate::fetch::async_download::DownloadJob;
use crate::paths_vars::PATHS;
use super::tui_theme::{self, tui_button_c, tui_separator, tui_dim, tui_number, tui_heading};
use super::types::{DeletionConfirmation, DownloadSource, ModStatus, AppTab};

impl super::app::ModUpdaterApp {
    pub(crate) fn render_deletion_modal(&mut self, ctx: &egui::Context) {
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
                        DeletionConfirmation::Datapack(world, key) => { tui_dim(ui, &format!("Borrar datapack '{}' del mundo '{}'?", key, world)); },
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
                                DeletionConfirmation::Datapack(world, key) => {
                                    let target_path = PATHS.saves_folder.join(&world).join("datapacks").join(&key);
                                    if std::fs::remove_file(&target_path).is_ok()
                                        || std::fs::remove_dir_all(&target_path).is_ok() {
                                        
                                        // Update in-memory state
                                        if let Some(packs) = self.world_datapacks.get_mut(&world) {
                                            packs.shift_remove(&key);
                                        }
                                        self.status_msg = format!("Datapack '{}' eliminado.", key);
                                    } else {
                                        self.status_msg = format!("Error al eliminar datapack '{}'.", key);
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

    pub(crate) fn render_download_modal(&mut self, ctx: &egui::Context) {
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
    }

    pub(crate) fn render_create_profile_modal(&mut self, ctx: &egui::Context) {
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
    }
}
