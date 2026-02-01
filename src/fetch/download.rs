use crossbeam_channel::{Sender, Receiver};
use std::thread;
use std::sync::Arc;
use std::env;
use reqwest::blocking::get;
use std::path::Path;
use crate::manage_mods::ModInfo;
use super::fetch_from_api;

#[derive(Debug, Clone)]
pub enum DownloadEvent {
    Resolving { key: String },
    Resolved { key: String },
    ResolvedInfo { key: String, confirmed_project_id: Option<String>, version_remote: Option<String> },
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
                        // Try to also obtain resolved IDs/versions for caching
                        let mut confirmed_project_id: Option<String> = None;
                        let mut version_remote: Option<String> = None;

                        // Try Modrinth first
                        // We use the search helper to get the ID if we successfully downloaded
                        // Just a quick re-verification or heuristic. 
                        // Since `find_mod_download` verified it, we can trust the ID if provided, 
                        // or we can search again to populate the cache ID.
                        
                        let hits = fetch_from_api::search_modrinth_project(mod_id_candidate.unwrap_or(&mi.name));
                        // Pick the best hit (first one usually if ID matched, or first name match)
                        if let Some(hit) = hits.first() {
                             confirmed_project_id = Some(hit.project_id.clone());
                             // Check if version exists to set version_remote
                             if let Some(v) = fetch_from_api::fetch_modrinth_version(&hit.project_id, &job.selected_version, &job.selected_loader) {
                                 version_remote = Some(job.selected_version.clone());
                                 // loaders = v.loaders; // This line was commented out as 'loaders' is not defined in this scope.
                             }
                        } else if let Some(cf_id) = fetch_from_api::fetch_curseforge_project_id(&mi.name, &cf_key) {
                            confirmed_project_id = Some(cf_id.to_string());
                            if let Some(cfile) = fetch_from_api::fetch_curseforge_version_file(cf_id, &job.selected_version, &job.selected_loader, &cf_key) {
                                version_remote = Some(cfile.file_name);
                            }
                        }

                        let _ = tx.send(DownloadEvent::Resolved { key: key.clone() });
                        let _ = tx.send(DownloadEvent::ResolvedInfo { key: key.clone(), confirmed_project_id: confirmed_project_id.clone(), version_remote: version_remote.clone() });
                        let _ = tx.send(DownloadEvent::Started { key: key.clone() });

                        // download file
                        let res = download_mod_file(&info.url, &job.output_folder, &info.filename);
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

pub fn download_mod_file(file_url: &str, output_folder: &str, filename: &str) -> Result<(), std::io::Error> {
    let mut resp_file = get(file_url).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    // Use Windows-friendly separators; ensure output folder exists
    let dest_path = format!("{}\\{}", output_folder, filename);
    if let Some(parent) = Path::new(&dest_path).parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut out_file = std::fs::File::create(&dest_path)?;
    std::io::copy(&mut resp_file, &mut out_file)?;
    Ok(())
}

