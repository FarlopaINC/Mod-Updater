use mods_updater::fetch::modrinth_api;
use mods_updater::fetch::curseforge_api;
use mods_updater::fetch::fetch_from_api;
use std::env;

/// Verifica que al arrancar, ambas APIs reportan capacidad disponible.
#[test]
fn test_both_apis_report_capacity_at_startup() {
    // Modrinth siempre debe empezar con capacidad (300 remaining, umbral 10)
    assert!(modrinth_api::has_capacity(), "Modrinth should have capacity at startup");

    // CurseForge token bucket empieza lleno
    assert!(curseforge_api::has_capacity(), "CurseForge token bucket should be full at startup");
}

/// Verifica que find_mod_download encuentra un mod conocido (Sodium en Modrinth).
/// Este test confirma que el flujo completo Modrinth → CurseForge funciona.
#[test]
fn test_find_mod_download_modrinth_primary() {
    let cf_key = env::var("CURSEFORGE_API_KEY").unwrap_or_default();
    
    // Sodium es un mod muy popular, debería encontrarse en Modrinth
    let result = fetch_from_api::find_mod_download(
        "Sodium",
        Some("AANobbMI"),  // Modrinth project ID conocido
        "1.20.1",
        "fabric",
        &cf_key,
    );

    assert!(result.is_some(), "Should find Sodium via load-balanced download");
    let info = result.unwrap();
    assert!(info.filename.contains("sodium"), "Downloaded file should be sodium");
    assert!(!info.url.is_empty(), "Should have a download URL");
}

/// Verifica que find_mod_download puede encontrar un mod solo por nombre
/// (sin ID), ejercitando el fallback de búsqueda.
#[test]
fn test_find_mod_download_by_name_only() {
    let cf_key = env::var("CURSEFORGE_API_KEY").unwrap_or_default();
    
    let result = fetch_from_api::find_mod_download(
        "Fabric API",
        None,  // Sin ID → forzar búsqueda por nombre
        "1.20.1",
        "fabric",
        &cf_key,
    );

    assert!(result.is_some(), "Should find Fabric API by name search");
    let info = result.unwrap();
    assert!(
        info.filename.to_lowercase().contains("fabric") || info.filename.to_lowercase().contains("api"),
        "Downloaded file should be fabric-related: got '{}'", info.filename
    );
}

/// Verifica que un mod inexistente devuelve None sin panic.
#[test]
fn test_find_mod_download_not_found() {
    let cf_key = env::var("CURSEFORGE_API_KEY").unwrap_or_default();
    
    let result = fetch_from_api::find_mod_download(
        "sdflkjhsdflkgjhsdlfkgjh_nonexistent_mod",
        None,
        "1.20.1",
        "fabric",
        &cf_key,
    );

    assert!(result.is_none(), "Should return None for nonexistent mod");
}
