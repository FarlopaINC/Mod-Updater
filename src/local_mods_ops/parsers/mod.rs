pub mod fabric_quilt;
pub mod forge_neoforge;

use std::path::Path;
use zip::ZipArchive;
use std::fs::File;
use crate::local_mods_ops::ModInfo;

/// Intenta parsear el JAR con todos los parsers disponibles, en orden.
/// Para añadir un nuevo loader: crear archivo, añadir `pub mod`, y una línea aquí.
pub fn try_all(zip: &mut ZipArchive<File>, path: &Path) -> Option<ModInfo> {
    fabric_quilt::try_parse(zip, path)
        .or_else(|| forge_neoforge::try_parse(zip, path))
    // Futuro:
    // .or_else(|| liteloader::try_parse(zip, path))
    // .or_else(|| risugamis::try_parse(zip, path))
}
