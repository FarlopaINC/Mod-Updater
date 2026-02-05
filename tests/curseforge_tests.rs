use mods_updater::fetch::fetch_from_api;
use std::env;

// Helper to ensure we have an API key for testing. 
// If not present, we skip the test to avoid false negatives in CI/local runs without keys.
fn get_api_key() -> Option<String> {
    env::var("CURSEFORGE_API_KEY").ok().filter(|k| !k.is_empty())
}

#[test]
fn test_search_basic() {
    let api_key = match get_api_key() {
        Some(k) => k,
        None => {
            println!("Skipping test_search_basic: CURSEFORGE_API_KEY not set");
            return;
        }
    };

    // Buscamos "JEI", un mod muy común
    let results = fetch_from_api::search_curseforge("JEI", &api_key, &None, &None, 0, 5);
    
    // Verificaciones
    assert!(!results.is_empty(), "Should return at least one result for 'JEI'");
    
    let found = results.iter().any(|m| m.name.contains("JEI") || m.name.contains("Just Enough Items"));
    assert!(found, "Should find 'JEI' or 'Just Enough Items' in results");
}

#[test]
fn test_search_with_filters() {
    let api_key = match get_api_key() {
        Some(k) => k,
        None => {
            println!("Skipping test_search_with_filters: CURSEFORGE_API_KEY not set");
            return;
        }
    };

    // Buscamos "Fabric API" para la version 1.20.1 y Loader Fabric
    let loader = Some("Fabric".to_string());
    let version = Some("1.20.1".to_string());
    
    let results = fetch_from_api::search_curseforge("Fabric API", &api_key, &loader, &version, 0, 5);

    assert!(!results.is_empty(), "Should return results for Fabric API 1.20.1");
    // Es difícil verificar programáticamente si *realmente* es de Fabric sin hacer más llamadas,
    // pero al menos verificamos que la API no devuelva error y traiga algo relevante.
    assert!(results[0].name.contains("Fabric"), "First result should likely contain 'Fabric'");
}

#[test]
fn test_search_no_results() {
    let api_key = match get_api_key() {
        Some(k) => k,
        None => {
            println!("Skipping test_search_no_results: CURSEFORGE_API_KEY not set");
            return;
        }
    };

    // Búsqueda absurda
    let results = fetch_from_api::search_curseforge("sdlfkjhsldkfjhsdlkfjh", &api_key, &None, &None, 0, 5);
    assert!(results.is_empty(), "Should return no results for random garbage string");
}

#[test]
fn test_fetch_project_id() {
    let api_key = match get_api_key() {
        Some(k) => k,
        None => {
            println!("Skipping test_fetch_project_id: CURSEFORGE_API_KEY not set");
            return;
        }
    };

    let id = fetch_from_api::fetch_curseforge_project_id("Just Enough Items (JEI)", &api_key);
    assert!(id.is_some(), "Should resolve ID for JEI");
    // JEI ID conocido es 238222, pero podría cambiar o ser otro fork, pero verificamos que sea un ID válido
    assert!(id.unwrap() > 0);
}

#[test]
fn test_fetch_version_file() {
    let api_key = match get_api_key() {
        Some(k) => k,
        None => {
            println!("Skipping test_fetch_version_file: CURSEFORGE_API_KEY not set");
            return;
        }
    };

    // JEI ID: 238222
    // Intentamos buscar una version específica
    let file = fetch_from_api::fetch_curseforge_version_file(
        238222, 
        "1.20.1", 
        "Forge", 
        &api_key
    );

    assert!(file.is_some(), "Should find a file for JEI 1.20.1 (Forge)");
    let f = file.unwrap();
    assert!(f.download_url.is_some(), "File should have a download URL");
    assert!(f.file_name.ends_with(".jar"), "File should be a JAR");
}

#[test]
fn test_fetch_file_invalid_params() {
    let api_key = match get_api_key() {
        Some(k) => k,
        None => {
            println!("Skipping test_fetch_file_invalid_params: CURSEFORGE_API_KEY not set");
            return;
        }
    };

    // JEI ID: 238222
    // Versión inexistente para Minecraft
    let file = fetch_from_api::fetch_curseforge_version_file(
        238222, 
        "0.0.1-beta-alpha-omega", 
        "Forge", 
        &api_key
    );

    assert!(file.is_none(), "Should NOT find a file for invalid version");
}

#[test]
fn test_invalid_api_key() {
    // Probamos explícitamente con una key mala
    let results = fetch_from_api::search_curseforge("JEI", "INVALID_KEY_12345", &None, &None, 0, 5);
    // Nuestra implementación actual devuelve vec![] en caso de error, y loguea a stdout.
    assert!(results.is_empty(), "Should return empty results with invalid API key");
}
