use crossbeam_channel::{Sender, Receiver};
use std::path::PathBuf;
use crate::local_mods_ops::{ModInfo, read_single_mod};
use crate::common::spawn_worker_pool;

#[derive(Debug, Clone)]
pub struct ReadJob {
    pub file_path: PathBuf,
}

#[derive(Debug, Clone)]
pub enum ReadEvent {
    Done { info: ModInfo },
    Error { path: PathBuf, msg: String },
}

pub fn spawn_read_workers(n: usize, rx: Receiver<ReadJob>, tx: Sender<ReadEvent>) {
    spawn_worker_pool(n, rx, move |job: ReadJob| {
        match read_single_mod(&job.file_path) {
            Ok(info) => {
                let filename = job.file_path.file_name().unwrap_or_default().to_string_lossy().to_string();
                crate::local_mods_ops::cache::upsert_mod(&filename, &info);
                let _ = tx.send(ReadEvent::Done { info });
            }
            Err(e) => {
                let _ = tx.send(ReadEvent::Error { path: job.file_path, msg: e });
            }
        }
    });
}
