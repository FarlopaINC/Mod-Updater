use crossbeam_channel::{Sender, Receiver};
use std::sync::Arc;
use crate::local_mods_ops::ModInfo;
use crate::fetch::fetch_from_api;
use crate::common::spawn_worker_pool;

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

    spawn_worker_pool(n, rx, move |job: DownloadJob| {
        let tx = tx_events.clone();
        let key = job.key.clone();
        let mi = job.modinfo.clone();

        let _ = tx.send(DownloadEvent::Resolving { key: key.clone() });

        let cf_key = crate::fetch::cf_api_key();

        // Resolver el ID del proyecto: confirmed > cache lookup > detected
        let resolved_id: Option<String> = mi.confirmed_project_id.clone()
            .or_else(|| {
                mi.detected_project_id.as_deref()
                    .and_then(crate::local_mods_ops::cache::get_confirmed_id)
            })
            .or(mi.detected_project_id.clone());

        match fetch_from_api::find_mod_download(&mi.name, resolved_id.as_deref(), &job.selected_version, &job.selected_loader, &cf_key) {
            Some(info) => {
                let confirmed_project_id = Some(info.project_id.clone());
                let version_remote = Some(info.version_remote.clone());

                let _ = tx.send(DownloadEvent::Resolved { key: key.clone() });
                let _ = tx.send(DownloadEvent::ResolvedInfo { key: key.clone(), confirmed_project_id, version_remote });
                let _ = tx.send(DownloadEvent::Started { key: key.clone() });

                let res = fetch_from_api::download_mod_file(&info.url, &job.output_folder, &info.filename);
                match res {
                    Ok(_) => { let _ = tx.send(DownloadEvent::Done { key: key.clone() }); }
                    Err(e) => { let _ = tx.send(DownloadEvent::Error { key: key.clone(), msg: format!("[ERROR]: Fallo descargando: {}", e) }); }
                }
            }
            None => {
                let _ = tx.send(DownloadEvent::Error { key: key.clone(), msg: format!("[ERROR]: No se encontró v{} ", job.selected_version) });
            }
        }
    });
}
