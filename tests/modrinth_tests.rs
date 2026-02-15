use mods_updater::fetch::modrinth_api;

#[test]
fn test_modrinth_search_basic() {
    // Buscamos "Sodium", muy popular
    let results = modrinth_api::search_modrinth_project("Sodium", &None, &None, 0, 5);
    
    assert!(!results.is_empty(), "Should return results for 'Sodium'");
    assert!(results[0].title.contains("Sodium"), "First result should be related to Sodium");
}

#[test]
fn test_modrinth_version_lookup() {
    // Sodium ID: AANobbMI
    // Buscamos versión para Fabric 1.20.1
    let version = modrinth_api::fetch_modrinth_version("AANobbMI", "1.20.1", "fabric");
    
    assert!(version.is_some(), "Should find Sodium for Fabric 1.20.1");
    let v = version.unwrap();
    assert!(v.first_file().is_some(), "Should have a file to download");
}

#[test]
fn test_modrinth_facets() {
    // Buscamos "Fabric API" filtrando por versión y loader
    let loader = Some("Fabric".to_string());
    let version = Some("1.20.1".to_string());
    
    let results = modrinth_api::search_modrinth_project("Fabric API", &loader, &version, 0, 5);
    
    assert!(!results.is_empty(), "Should return results");
    assert!(results[0].title.contains("Fabric API"), "Should find Fabric API");
}

#[test]
fn test_modrinth_has_capacity() {
    // Al inicio siempre debería tener capacidad (300 requests, umbral 10)
    assert!(modrinth_api::has_capacity(), "Should have capacity at startup");
}
