use super::search_provider::{ContentSearchProvider, ContentType, SearchRequest, UnifiedSearchResult};
use super::{modrinth_api, curseforge_api};

/// Provider de búsqueda para Mods — implementa el trait genérico.
pub struct ModSearchProvider;

impl ContentSearchProvider for ModSearchProvider {
    fn content_type(&self) -> ContentType {
        return ContentType::Mod;
    }

    fn search(&self, req: &SearchRequest) -> Vec<UnifiedSearchResult> {
        let mut results = Vec::new();
        let extra_facets = vec!["[\"project_type:mod\"]".to_string()];

        // 1. Modrinth Search
        let modrinth_hits = modrinth_api::search_modrinth_project(
            &req.query, &req.loader, &req.version, req.offset, req.limit, &extra_facets,
        );
        for hit in modrinth_hits {
            results.push(UnifiedSearchResult {
                name: hit.title,
                slug: hit.slug,
                description: hit.description.unwrap_or_default(),
                icon_url: hit.icon_url,
                author: hit.author,
                modrinth_id: Some(hit.project_id),
                curseforge_id: None,
                dependencies: None,
                fetching_dependencies: false,
                content_type: ContentType::Mod,
            });
        }

        // 2. CurseForge Search
        let cf_key = crate::fetch::cf_api_key();
        if !cf_key.is_empty() {
            let curse_hits = curseforge_api::search_curseforge(
                &req.query, &cf_key, &req.loader, &req.version, req.offset, req.limit, Some(6),
            );
            for hit in curse_hits {
                // Dedup by slug or name
                if let Some(existing) = results.iter_mut().find(|r| r.slug == hit.slug || r.name == hit.name) {
                    existing.curseforge_id = Some(hit.id);
                    if existing.icon_url.is_none() {
                        existing.icon_url = hit.logo.map(|l| l.thumbnail_url);
                    }
                    if existing.description.is_empty() {
                        existing.description = hit.summary.unwrap_or_default();
                    }
                } else {
                    results.push(UnifiedSearchResult {
                        name: hit.name,
                        slug: hit.slug,
                        description: hit.summary.unwrap_or_default(),
                        icon_url: hit.logo.map(|l| l.thumbnail_url),
                        author: "Unknown (CF)".to_string(),
                        modrinth_id: None,
                        curseforge_id: Some(hit.id),
                        dependencies: None,
                        fetching_dependencies: false,
                        content_type: ContentType::Mod,
                    });
                }
            }
        }
        results
    }

    fn supports_loader_filter(&self) -> bool { true }
    fn supports_version_filter(&self) -> bool { true }

    fn fetch_versions(&self, project_id: &str, loader: &str, game_version: &str) -> Vec<crate::fetch::search_provider::ProjectVersion> {
        // Try parsing project_id as u32 to see if it's CurseForge
        if let Ok(cf_id) = project_id.parse::<u32>() {
            let cf_key = crate::fetch::cf_api_key();
            if !cf_key.is_empty() {
                return curseforge_api::fetch_curseforge_project_versions(cf_id, game_version, loader, &cf_key, &ContentType::Mod);
            }
        }
        
        // Otherwise assume Modrinth
        modrinth_api::fetch_modrinth_project_versions(project_id, loader, game_version, &ContentType::Mod)
    }
}
