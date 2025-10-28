use eframe::{egui, egui::CentralPanel};
use eframe::egui::ScrollArea;
use crate::manage_mods::{get_minecraft_versions, ModInfo};
use indexmap::IndexMap;
use std::ops::{Deref, DerefMut};
use crossbeam_channel::{unbounded, Sender, Receiver};
use crate::download::{spawn_workers, DownloadJob, DownloadEvent};

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

pub struct ModUpdaterApp {
    pub mods: IndexMap<String, UiModInfo>,
    pub mc_versions: Vec<String>,
    pub selected_mc_version: String,
    tx_jobs: Sender<DownloadJob>,
    rx_events: Receiver<DownloadEvent>,
}

impl ModUpdaterApp {
    pub fn new(mods: IndexMap<String, ModInfo>) -> Self {
        let mc_versions = get_minecraft_versions(r"C:\Users\Mario\AppData\Roaming\.minecraft\versions\version_manifest_v2.json");
        let selected_mc_version = mc_versions.get(0).cloned().unwrap_or_else(|| "1.20.2".to_string());
        let ui_mods: IndexMap<String, UiModInfo> = mods.into_iter().map(|(k, v)| (k, UiModInfo::from(v))).collect();

        // create channels and spawn workers
        let (tx_jobs, rx_jobs) = unbounded();
        let (tx_events, rx_events) = unbounded();
        spawn_workers(4, rx_jobs, tx_events);

        Self { mods: ui_mods, mc_versions, selected_mc_version, tx_jobs, rx_events }
    }
}

impl eframe::App for ModUpdaterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Versión de Minecraft:");
                egui::ComboBox::from_id_salt("mc-version-box")
                    .selected_text(&self.selected_mc_version)
                    .show_ui(ui, |ui| {
                        for v in &self.mc_versions {
                            ui.selectable_value(&mut self.selected_mc_version, v.clone(), v);
                        }
                        ui.separator();
                        ui.label("Personalizada:");
                        ui.text_edit_singleline(&mut self.selected_mc_version);
                    });
            });

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("💾 Actualizar seleccionados").clicked() {
                        // Encolar trabajos para los mods seleccionados
                        for (k, m) in self.mods.clone().into_iter() {
                            if m.selected {
                                crate::manage_mods::prepare_output_folder(&self.selected_mc_version);
                                let output_folder = format!(r"C:\Users\Mario\AppData\Roaming\.minecraft\modpacks\mods{}", self.selected_mc_version);
                                let job = DownloadJob {
                                    key: k.clone(),
                                    modinfo: m.inner.clone(),
                                    output_folder,
                                    selected_version: self.selected_mc_version.clone()
                                };
                                let _ = self.tx_jobs.send(job);
                            }
                        }
                    }
                });
            });

            ui.add_space(8.0); // Espacio entre el botón y la lista
            ui.label("Mods instalados");
            ScrollArea::vertical().show(ui, |ui| {
                let keys: Vec<String> = self.mods.keys().cloned().collect();
                for key in keys {
                    if let Some(m) = self.mods.get_mut(&key) {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut m.selected, "");
                            ui.label(&m.name);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add_space(8.0); // Margen derecho para el scrollbar
                                match &m.status {
                                    ModStatus::Idle => {},
                                    ModStatus::Resolving => { ui.label("⌛ Resolviendo..."); },
                                    ModStatus::Downloading(progress) => { ui.label(format!("📥 {}%", (progress * 100.0) as i32)); },
                                    ModStatus::Done => { ui.label("✅ Completado"); },
                                    ModStatus::Error(msg) => { ui.label(format!("❌ Error: {}", msg)); },
                                }
                            });
                        });
                    }
                }
            });
            ui.separator();

            // Procesar eventos entrantes desde los workers
            for ev in self.rx_events.try_iter() {
                match ev {
                    DownloadEvent::Resolving { key } => {
                        if let Some(m) = self.mods.get_mut(&key) { m.status = ModStatus::Resolving; }
                    }
                    DownloadEvent::Resolved { key, project_id: _ } => {
                        if let Some(m) = self.mods.get_mut(&key) { m.status = ModStatus::Idle; }
                    }
                    DownloadEvent::Started { key } => {
                        if let Some(m) = self.mods.get_mut(&key) { m.status = ModStatus::Downloading(0.0); }
                    }
                    DownloadEvent::Progress { key, progress } => {
                        if let Some(m) = self.mods.get_mut(&key) { m.progress = progress; m.status = ModStatus::Downloading(progress); }
                    }
                    DownloadEvent::Done { key, path: _ } => {
                        if let Some(m) = self.mods.get_mut(&key) { m.status = ModStatus::Done; m.progress = 1.0; }
                    }
                    DownloadEvent::Error { key, msg } => {
                        if let Some(m) = self.mods.get_mut(&key) { m.status = ModStatus::Error(msg); }
                    }
                }
            }
        });
    }
}
