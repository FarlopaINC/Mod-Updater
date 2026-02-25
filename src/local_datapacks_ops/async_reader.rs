use crossbeam_channel::{Sender, Receiver};
use std::path::PathBuf;
use super::models::DatapackInfo;
use crate::common::spawn_worker_pool;

#[derive(Debug, Clone)]
pub struct DatapackReadJob {
    pub file_path: PathBuf,
    pub world_name: String,
}

#[derive(Debug, Clone)]
pub enum DatapackReadEvent {
    Done { world_name: String, info: DatapackInfo },
    Error { world_name: String, path: PathBuf, msg: String },
}

pub fn spawn_datapack_read_workers(n: usize, rx: Receiver<DatapackReadJob>, tx: Sender<DatapackReadEvent>) {
    spawn_worker_pool(n, rx, move |job: DatapackReadJob| {
        let filename = job.file_path.file_name().unwrap_or_default().to_string_lossy().to_string();
        match super::scanner::read_single_datapack(&job.file_path) {
            Ok(mut info) => {
                // Attach file metadata
                if let Ok(meta) = std::fs::metadata(&job.file_path) {
                    info.file_size_bytes = Some(meta.len());
                    info.file_mtime_secs = Some(
                        meta.modified()
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                            .duration_since(std::time::SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs()
                    );
                }
                let _ = tx.send(DatapackReadEvent::Done { world_name: job.world_name, info });
            }
            Err(msg) => {
                let _ = tx.send(DatapackReadEvent::Error {
                    world_name: job.world_name,
                    path: job.file_path,
                    msg,
                });
            }
        }
    });
}
