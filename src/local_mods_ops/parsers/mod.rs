pub mod fabric_quilt;
pub mod forge_neoforge;

use std::io::Read;
use std::path::Path;
use zip::ZipArchive;
use std::fs::File;
use crate::local_mods_ops::ModInfo;

/// Lee el contenido de una entrada del ZIP por nombre de archivo.
/// Hace una búsqueda exacta por ruta completa O por nombre de archivo (basename).
/// Normaliza separadores para compatibilidad Windows/Unix.
pub fn read_zip_entry(zip: &mut ZipArchive<File>, target: &str) -> Option<String> {
    for i in 0..zip.len() {
        if let Ok(mut entry) = zip.by_index(i) {
            let entry_name = entry.name().replace('\\', "/");
            let matches = entry_name == target
                || entry_name.ends_with(&format!("/{target}"));
            if matches {
                let mut buf = String::new();
                if entry.read_to_string(&mut buf).is_ok() && !buf.is_empty() {
                    return Some(buf);
                }
            }
        }
    }
    None
}

/// Intenta parsear el JAR con todos los parsers disponibles, en orden.
/// Para añadir un nuevo loader: crear archivo, añadir `pub mod`, y una línea aquí.
pub fn try_all(zip: &mut ZipArchive<File>, path: &Path) -> Option<ModInfo> {
    fabric_quilt::try_parse(zip, path)
        .or_else(|| forge_neoforge::try_parse(zip, path))
    // Futuro:
    // .or_else(|| liteloader::try_parse(zip, path))
}
