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

use rayon::prelude::*;

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
                     // 4. Intento 3: Copia Física Paralela (Fallback robusto)
                     // Limpiamos lo que haya podido dejar el intento de hardlinks
                     if target.exists() { let _ = fs::remove_dir_all(target); }
                     
                     match copy_modpack_parallel(source, target) {
                        Ok(_) => {
                            let _ = write_active_marker(modpack);
                            Ok(format!("Mods cambiados a '{}' usando Copia Paralela (fallback).", modpack))
                        },
                        Err(e_cp) => Err(format!("Fallo total al cambiar mods.\nSymlink: {:?}\nHardLink: {:?}\nCopy: {:?}", e_sym, e_hl, e_cp)),
                     }
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
    // Para hardlinks, igual necesitamos recorrer y linkear uno a uno.
    // No podemos usar rayon fácilmente porque create_dir no es thread safe si colisionan, 
    // pero la creación de links es rápida. Lo haremos secuencial o híbrido.
    // Haremos secuencial la estructura de directorios y luego los links.
    
    // De hecho, podemos reusar la logica de collect para separar dirs y files.
    let mut files = Vec::new();
    collect_copy_ops(src, dst, &mut files)?;
    
    // Intentar crear hardlinks
    for (from, to) in files {
        fs::hard_link(&from, &to)?;
    }
    Ok(())
}

pub fn copy_modpack_parallel(src: &Path, dst: &Path) -> std::io::Result<()> {
    let mut files = Vec::new();
    collect_copy_ops(src, dst, &mut files)?;
    
    // Usar Rayon para copiar archivos en paralelo
    files.par_iter().try_for_each(|(from, to)| {
        fs::copy(from, to).map(|_| ())
    })
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
