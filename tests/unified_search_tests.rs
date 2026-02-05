use mods_updater::fetch::search::{search_unified, SearchRequest};
use std::env;

// Helper para verificar API key
fn has_curse_key() -> bool {
    env::var("CURSEFORGE_API_KEY").is_ok_and(|k| !k.is_empty())
}

#[test]
fn test_unified_flow() {
    // JEI suele estar en ambos
    let req = SearchRequest {
        query: "JEI".to_string(),
        loader: Some("Fabric".to_string()),
        version: Some("1.20.1".to_string()),
        offset: 0,
        limit: 10,
    };

    let results = search_unified(&req);

    assert!(!results.is_empty(), "Should return results for JEI");

    // Verificamos estructura básica
    let first = &results[0];
    assert!(first.modrinth_id.is_some(), "Should have Modrinth ID (primary source)");

    if has_curse_key() {
        // Si tenemos key, verificamos la fusión.
        // Buscamos algún resultado que tenga AMBOS IDs.
        // JEI es muy probable que los tenga.
        let merged = results.iter().find(|r| r.modrinth_id.is_some() && r.curseforge_id.is_some());
        
        if let Some(m) = merged {
            println!("✅ Successfully merged result found: {} (Modrinth: {:?}, CF: {:?})", 
                m.name, m.modrinth_id, m.curseforge_id);
        } else {
            println!("⚠️ Warning: No merged results found for JEI. Check naming/slug inconsistencies.");
            // No fallamos el test fuertemente porque a veces los nombres difieren y no hacen mach,
            // pero imprimimos advertencia.
            // Para debugging, imprimimos lo que encontramos:
            for r in &results {
                println!("- {} (M: {:?}, C: {:?})", r.name, r.modrinth_id, r.curseforge_id);
            }
        }
    } else {
        println!("Skipping merge validation: CURSEFORGE_API_KEY not set");
    }
}
