use crate::paths_vars::PATHS;
use std::path::{Path, PathBuf};
use std::fs;

#[cfg(target_family = "unix")]
use std::os::unix::fs::symlink as symlink;

#[cfg(target_family = "windows")]
use std::os::windows::fs::symlink_dir as symlink;

pub fn prepare_output_folder(version: &str) {
    let base_path = PATHS.modpacks_folder.to_string_lossy().to_string();
    // Crear carpeta base si no existe
    if !Path::new(&base_path).exists() {
        fs::create_dir_all(&base_path).expect("ERROR: No se pudo crear la carpeta modpacks");
    }
    
    let output_folder = format!("{}/mods{}", base_path, version);
    if !Path::new(&output_folder).exists() {
        fs::create_dir(&output_folder).expect("ERROR: No se pudo crear la carpeta de versión");
    } 
}   

pub fn change_mods(modpack: &str) -> Result<String, String> {
    let target = &PATHS.mods_folder;
    let source = &PATHS.modpacks_folder.join(modpack);

    // Si ya existe un enlace simbólico, intentamos eliminarlo; si no es symlink, continuamos
    if let Ok(metadata) = std::fs::symlink_metadata(target) {
        if metadata.file_type().is_symlink() {
            // intentar eliminar como archivo primero, si falla, intentar como dir
            let _ = std::fs::remove_file(target).or_else(|_| std::fs::remove_dir(target));
        }
    }

    // Intentar crear enlace simbólico / junction (rápido)
    match symlink(source, target) {
        Ok(_) => {
            let _ = write_active_marker(modpack);
            return Ok(format!("Mods cambiados a '{}' usando enlace/junction.", modpack));
        }
        Err(e) => {
            // Fallback: intentar copiar el modpack preservando el origen (no usar rename)
            match copy_modpack_all(source, target) {
                Ok(()) => {
                    let _ = write_active_marker(modpack);
                    Ok(format!("Mods cambiados a '{}' usando fallback (copia preservando original).", modpack))
                }
                Err(e2) => Err(format!("No se pudo cambiar mods: symlink/junction falló ({:?}), fallback (copia) falló ({:?})", e, e2)),
            }
        }
    }
}

pub fn copy_modpack_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    if (&PATHS.mods_folder).exists() {
        fs::remove_dir_all(&PATHS.mods_folder)?;
    }
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_modpack_all(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(&from, &to)?;
        } else {
            // Ignorar otros tipos (symlinks, etc.)
        }
    }
    Ok(())
}

// Marker file helpers
fn active_marker_path() -> PathBuf {
    PATHS.base_game_folder.join("mods_updater_active_modpack.txt")
}

fn write_active_marker(modpack: &str) -> std::io::Result<()> {
    let p = active_marker_path();
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(p, modpack.as_bytes())
}

pub fn read_active_marker() -> Option<String> {
    let p = active_marker_path();
    if p.exists() {
        if let Ok(s) = fs::read_to_string(p) {
            return Some(s.trim().to_string());
        }
    }
    None
}
