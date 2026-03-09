/// Módulo de utilidades compartidas entre local_mods_ops, fetch y futuros módulos (datapacks, etc.).
/// Aquí solo viven abstracciones genéricas que NO dependen de ninguna lógica de dominio.

use crossbeam_channel::Receiver;
use std::thread;

/// Lanza `n` hilos trabajadores que consumen trabajos del canal `rx`
/// y los procesan con el `handler` proporcionado.
///
/// Úsalo en lugar de duplicar el patrón `Arc<tx> + for _ in 0..n { thread::spawn(...) }`.
///
/// # Ejemplo
/// ```ignore
/// spawn_worker_pool(4, rx, |job: MyJob| {
///     // procesar job...
/// });
/// ```
pub fn spawn_worker_pool<Job, F>(n: usize, rx: Receiver<Job>, handler: F)
where
    Job: Send + 'static,
    F: Fn(Job) + Send + Clone + 'static,
{
    for _ in 0..n {
        let rx = rx.clone();
        let handler = handler.clone();
        thread::spawn(move || {
            while let Ok(job) = rx.recv() {
                handler(job);
            }
        });
    }
}

pub fn calculate_worker_count(task_count: usize) -> usize {
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    
    // Dynamic worker calculation:
    // - Small mod counts: 1 worker per mod (min(mods, max_workers))
    // - Large mod counts: Up to 8 workers per cpu, capped at 64
    let max_workers = (cpus * 8).clamp(4, 64);
    std::cmp::min(task_count, max_workers).max(1)
}
