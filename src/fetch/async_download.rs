use crossbeam_channel::{Sender, Receiver};
use std::thread;
use std::sync::Arc;
use std::env;
use crate::manage_mods::ModInfo;
use super::fetch_from_api;


#[derive(Debug, Clone)]
pub enum DownloadEvent {
    Resolving { key: String },
    Resolved { key: String },
    ResolvedInfo { key: String, confirmed_project_id: Option<String>, version_remote: Option<String> },
    Started { key: String },
    Progress(String, f32),
    Done { key: String },
    Error { key: String, msg: String },
}

#[derive(Debug, Clone)]
pub struct DownloadJob {
    pub key: String,
    pub modinfo: ModInfo,
    pub output_folder: String,
    pub selected_version: String,
    pub selected_loader: String,
}

pub fn spawn_workers(n: usize, rx: Receiver<DownloadJob>, tx_events: Sender<DownloadEvent>) {
    let tx_events = Arc::new(tx_events);
    for _ in 0..n {
        let rx = rx.clone();
        let tx = tx_events.clone();
        thread::spawn(move || {
            while let Ok(job) = rx.recv() {
                let key = job.key.clone();
                let mi = job.modinfo.clone();
                // Use the unified fetch API (Modrinth primary, CurseForge fallback)
                let _ = tx.send(DownloadEvent::Resolving { key: key.clone() });

                // Get CurseForge API key from env if present (optional)
                let cf_key = env::var("CURSEFORGE_API_KEY").unwrap_or_default();

                let mod_id_candidate = mi.confirmed_project_id.as_deref().or(mi.detected_project_id.as_deref());

                match fetch_from_api::find_mod_download(&mi.name, mod_id_candidate, &job.selected_version, &job.selected_loader, &cf_key) {
                     Some(info) => {
                        let confirmed_project_id = Some(info.project_id.clone());
                        let version_remote = Some(info.version_remote.clone());

                        let _ = tx.send(DownloadEvent::Resolved { key: key.clone() });
                        let _ = tx.send(DownloadEvent::ResolvedInfo { key: key.clone(), confirmed_project_id, version_remote });
                        let _ = tx.send(DownloadEvent::Started { key: key.clone() });

                        // download file
                        let res = fetch_from_api::download_mod_file(&info.url, &job.output_folder, &info.filename);
                        match res {
                            Ok(_) => {
                                let _ = tx.send(DownloadEvent::Done { key: key.clone() });
                            }
                            Err(e) => {
                                let _ = tx.send(DownloadEvent::Error { key: key.clone(), msg: format!("[ERROR]: Fallo descargando: {}", e) });
                            }
                        }
                    }
                    None => {
                        let _ = tx.send(DownloadEvent::Error { key: key.clone(), msg: format!("[ERROR]: No se encontró v{} ", job.selected_version) });
                    }
                }
            }
        });
    }
}



