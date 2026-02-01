use crossbeam_channel::{Sender, Receiver};
use std::thread;
use std::sync::Arc;
use std::path::PathBuf;
use crate::manage_mods::{ModInfo, read_single_mod};

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
    let tx = Arc::new(tx);
    for _ in 0..n {
        let rx = rx.clone();
        let tx = tx.clone();
        thread::spawn(move || {
            while let Ok(job) = rx.recv() {
                match read_single_mod(&job.file_path) {
                    Ok(info) => {
                        let _ = tx.send(ReadEvent::Done { info });
                    }
                    Err(e) => {
                         let _ = tx.send(ReadEvent::Error { path: job.file_path, msg: e });
                    }
                }
            }
        });
    }
}
