use eframe::egui::{self, ScrollArea, SidePanel};

use crate::profiles::save_profiles;
use super::tui_theme::{self, tui_button, tui_button_c, tui_separator, tui_dim, tui_number};
use super::types::{DeletionConfirmation, DownloadSource, SearchSource};

impl super::app::ModUpdaterApp {
    pub(crate) fn render_profiles_side(&mut self, ctx: &egui::Context) {
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

    pub(crate) fn render_profiles_center(&mut self, ui: &mut egui::Ui) {
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
}
