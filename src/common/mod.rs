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
