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

/// Lee el contenido binario de una entrada del ZIP.
pub fn read_zip_entry_bytes(zip: &mut ZipArchive<File>, target: &str) -> Option<Vec<u8>> {
    for i in 0..zip.len() {
        if let Ok(mut entry) = zip.by_index(i) {
            let entry_name = entry.name().replace('\\', "/");
            let matches = entry_name == target
                || entry_name.ends_with(&format!("/{target}"));
            if matches {
                let mut buf = Vec::new();
                if entry.read_to_end(&mut buf).is_ok() && !buf.is_empty() {
                    return Some(buf);
                }
            }
        }
    }
    None
}

/// Extrae bytes del icono, los dimensiona a 64x64 y los guarda.
pub fn extract_and_save_icon(zip: &mut ZipArchive<File>, icon_path: &str, project_id: &str) -> bool {
    if let Some(bytes) = read_zip_entry_bytes(zip, icon_path) {
        if let Ok(img) = image::load_from_memory(&bytes) {
            // Resize if too big, to save memory and disk space
            let resized = if img.width() > 64 || img.height() > 64 {
                img.resize(64, 64, image::imageops::FilterType::Triangle)
            } else {
                img
            };
            let save_path = crate::paths_vars::PATHS.icons_folder.join(format!("{}.png", project_id));
            if resized.save_with_format(save_path, image::ImageFormat::Png).is_ok() {
                return true;
            }
        }
    }
    false
}

/// Intenta parsear el JAR con todos los parsers disponibles, en orden.
/// Para añadir un nuevo loader: crear archivo, añadir `pub mod`, y una línea aquí.
pub fn try_all(zip: &mut ZipArchive<File>, path: &Path) -> Option<ModInfo> {
    fabric_quilt::try_parse(zip, path)
        .or_else(|| forge_neoforge::try_parse(zip, path))
    // Futuro:
    // .or_else(|| liteloader::try_parse(zip, path))
}
