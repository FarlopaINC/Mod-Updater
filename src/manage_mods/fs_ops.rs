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

    // 1. Limpieza: Si ya existe un enlace simbólico, intentamos eliminarlo
    if let Ok(metadata) = std::fs::symlink_metadata(target) {
        if metadata.file_type().is_symlink() {
            let _ = std::fs::remove_file(target).or_else(|_| std::fs::remove_dir(target));
        } else if metadata.is_dir() {
             // Si es un directorio real, lo borramos entero para asegurar estado limpio
             let _ = std::fs::remove_dir_all(target);
        }
    }

    // 2. Intento 1: Symlink / Junction (Rápido, pero puede pedir permisos)
    match symlink(source, target) {
        Ok(_) => {
            let _ = write_active_marker(modpack);
            return Ok(format!("Mods cambiados a '{}' usando enlace/junction.", modpack));
        }
        Err(e_sym) => {
             // 3. Intento 2: Hard Links (Rápido, sin permisos de admin, pero misma partición)
             match copy_modpack_hardlinks(source, target) {
                 Ok(_) => {
                     let _ = write_active_marker(modpack);
                     return Ok(format!("Mods cambiados a '{}' usando Hard Links (rápido).", modpack));
                 },
                 Err(e_hl) => {
                     // 4. Fallo total
                     Err(format!("Fallo al cambiar mods (se requieren permisos o misma partición).\nSymlink: {:?}\nHardLink: {:?}", e_sym, e_hl))
                 }
             }
        }
    }
}

// Recoleta operaciones de copia recursivamente
fn collect_copy_ops(src: &Path, dst: &Path, ops: &mut Vec<(PathBuf, PathBuf)>) -> std::io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        
        if ty.is_dir() {
            collect_copy_ops(&from, &to, ops)?;
        } else if ty.is_file() {
            ops.push((from, to));
        }
    }
    Ok(())
}

pub fn copy_modpack_hardlinks(src: &Path, dst: &Path) -> std::io::Result<()> {
    let mut files = Vec::new();
    collect_copy_ops(src, dst, &mut files)?;
    
    // Intentar crear hardlinks
    for (from, to) in files {
        fs::hard_link(&from, &to)?;
    }
    Ok(())
}

// Marker file helpers
fn active_marker_path() -> PathBuf {
    return PATHS.base_game_folder.join("mods_updater_active_modpack.txt");
}

fn write_active_marker(modpack: &str) -> std::io::Result<()> {
    let p = active_marker_path();
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }
    return fs::write(p, modpack.as_bytes());
}

pub fn read_active_marker() -> Option<String> {
    let p = active_marker_path();
    if p.exists() {
        if let Ok(s) = fs::read_to_string(p) {
            return Some(s.trim().to_string());
        }
    }
    return None;
}

/// Elimina archivos `.part` huérfanos de descargas interrumpidas.
/// Escanea recursivamente la carpeta de modpacks.
pub fn cleanup_partial_downloads() {
    let dir = &PATHS.modpacks_folder;
    if !dir.exists() {
        return;
    }
    cleanup_part_files_recursive(dir);
}

fn cleanup_part_files_recursive(dir: &Path) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            cleanup_part_files_recursive(&path);
        } else if path.extension().and_then(|e| e.to_str()) == Some("part") {
            println!("🧹 Eliminando descarga parcial: {}", path.display());
            let _ = fs::remove_file(&path);
        }
    }
}
