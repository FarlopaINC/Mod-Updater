use crossbeam_channel::{Sender, Receiver};
use std::thread;
use std::sync::Arc;
use crate::manage_mods::ModInfo;
use crate::utils::modrinth_api;

#[derive(Debug, Clone)]
pub enum DownloadEvent {
    Resolving { key: String },
    Resolved { key: String },
    Started { key: String },
    Done { key: String },
    Error { key: String, msg: String },
}

#[derive(Debug, Clone)]
pub struct DownloadJob {
    pub key: String,
    pub modinfo: ModInfo,
    pub output_folder: String,
    pub selected_version: String,
}

pub fn spawn_workers(n: usize, rx: Receiver<DownloadJob>, tx_events: Sender<DownloadEvent>) {
    let tx_events = Arc::new(tx_events);
    for _ in 0..n {
        let rx = rx.clone();
        let tx = tx_events.clone();
        thread::spawn(move || {
            while let Ok(job) = rx.recv() {
                let key = job.key.clone();
                let mut mi = job.modinfo.clone();
                // Resolve project_id if missing
                if mi.confirmed_project_id.is_none() {
                    let _ = tx.send(DownloadEvent::Resolving { key: key.clone() });
                    if let Some(pid) = modrinth_api::fetch_modrinth_project_id(&mi.name) {
                        mi.confirmed_project_id = Some(pid.clone());
                        let _ = tx.send(DownloadEvent::Resolved { key: key.clone() });
                    } else {
                        let _ = tx.send(DownloadEvent::Error { key: key.clone(), msg: format!("No se pudo resolver project_id para {}", mi.name) });
                        continue;
                    }
                }

                // At this point we should have confirmed_project_id and selected version should be
                // resolved by caller into a download URL; for now, fetch version and get file
                if let Some(pid) = mi.confirmed_project_id.clone() {
                    let _ = tx.send(DownloadEvent::Started { key: key.clone() });
                    match modrinth_api::fetch_modrinth_version(&pid, &mi.version_remote.clone().unwrap_or(job.selected_version.clone())) {
                        Some(v) => {
                            if let Some(file) = v.first_file() {
                                // download file
                                let filename = &file.filename;
                                let res = modrinth_api::download_mod_file(&file.url, &job.output_folder, filename);
                                match res {
                                    Ok(_) => {
                                        let _ = tx.send(DownloadEvent::Done { key: key.clone() });
                                        // update cache? skipping here; caller should update
                                    }
                                    Err(e) => {
                                        let _ = tx.send(DownloadEvent::Error { key: key.clone(), msg: format!("Error descargando: {}", e) });
                                    }
                                }
                            } else {
                                let _ = tx.send(DownloadEvent::Error { key: key.clone(), msg: "No se encontró archivo en la versión".to_string() });
                            }
                        }
                        None => {
                            let _ = tx.send(DownloadEvent::Error { key: key.clone(), msg: "No se encontró versión compatible".to_string() });
                        }
                    }
                }
            }
        });
    }
}
