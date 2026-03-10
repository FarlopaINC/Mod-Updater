/// Motor de búsqueda genérico basado en traits.
/// Equivalente a una clase abstracta de C++ — define la interfaz
/// que deben implementar todos los tipos de contenido buscable.

/// Tipo de contenido que se busca (para filtrado en APIs)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentType {
    Mod,
    Datapack,
    // Futuras extensiones:
    // Shader,
    // ResourcePack,
}

impl ContentType {
    pub fn display_name(&self) -> &str {
        match self {
            ContentType::Mod => "Mods",
            ContentType::Datapack => "Datapacks",
        }
    }

    /// Todos los tipos disponibles, para iterar en la UI
    pub fn all() -> &'static [ContentType] {
        &[ContentType::Mod, ContentType::Datapack]
    }
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Petición de búsqueda unificada (antes vivía en single_mod_search.rs)
#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub query: String,
    pub loader: Option<String>,
    pub version: Option<String>,
    pub offset: u32,
    pub limit: u32,
    pub content_type: ContentType,
}

/// Resultado de búsqueda unificado (antes vivía en single_mod_search.rs)
#[derive(Debug, Clone)]
pub struct UnifiedSearchResult {
    pub name: String,
    pub slug: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub author: String,

    // Source specific info
    pub modrinth_id: Option<String>,
    pub curseforge_id: Option<u32>,

    // UI Dependency Viewer
    pub dependencies: Option<Vec<String>>,
    pub fetching_dependencies: bool,

    // Tipo de contenido que originó este resultado
    pub content_type: ContentType,
}

#[derive(Debug, Clone)]
pub struct ProjectVersion {
    pub id: String,          // The version ID (Modrinth version ID, CurseForge file ID)
    pub name: String,        // Version name (e.g. "Sodium 0.5.3")
    pub version_number: String, // e.g. "0.5.3"
    pub release_type: String, // 'R' for release, 'B' for beta, 'A' for alpha
    pub game_versions: Vec<String>, // e.g. ["1.20.1"]
    pub date_published: String, // Displayable date
}

/// Trait que define lo que debe implementar cada tipo de contenido buscable.
/// Equivalente a una clase abstracta en C++.
pub trait ContentSearchProvider: Send + Sync {
    /// Tipo de contenido que busca este provider
    fn content_type(&self) -> ContentType;

    /// Ejecuta la búsqueda y devuelve resultados unificados
    fn search(&self, req: &SearchRequest) -> Vec<UnifiedSearchResult>;

    /// Si el tipo soporta filtrar por loader (Fabric, Forge, etc.)
    fn supports_loader_filter(&self) -> bool;

    /// Si el tipo soporta filtrar por versión de MC
    fn supports_version_filter(&self) -> bool;

    /// Recupera las versiones de un proyecto específico (para la selección manual)
    fn fetch_versions(&self, project_id: &str, loader: &str, game_version: &str) -> Vec<ProjectVersion> {
        // Implementación por defecto retorna vacío (hasta que los modifiquen)
        let _ = (project_id, loader, game_version);
        Vec::new()
    }
}
