pub mod async_download;
pub mod fetch_from_api;
pub mod single_mod_search;
pub mod modrinth_api;
pub mod curseforge_api;

/// Devuelve la API key de CurseForge desde la variable de entorno, o cadena vacía si no está definida.
/// Usar esta función evita repetir `std::env::var("CURSEFORGE_API_KEY").unwrap_or_default()` en toda la base de código.
pub fn cf_api_key() -> String {
    std::env::var("CURSEFORGE_API_KEY").unwrap_or_default()
}