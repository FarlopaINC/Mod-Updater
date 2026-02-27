use eframe::egui::{self, ScrollArea};
use indexmap::IndexMap;

use crate::local_datapacks_ops::{list_worlds, DatapackReadJob};
use crate::paths_vars::PATHS;
use super::tui_theme::{self, tui_button, tui_heading, tui_dim};

impl super::app::ModUpdaterApp {
    pub(crate) fn load_all_datapacks(&mut self) {
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

    pub(crate) fn render_datapacks_center(&mut self, ui: &mut egui::Ui) {
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

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if super::tui_theme::tui_button_c(ui, "DEL", super::tui_theme::NEON_RED).clicked() {
                                        self.deletion_confirmation = crate::ui::types::DeletionConfirmation::Datapack(
                                            world.clone(),
                                            key.clone()
                                        );
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
