use super::search_provider::{ContentSearchProvider, ContentType, SearchRequest, UnifiedSearchResult};
use super::{modrinth_api, curseforge_api};

/// Provider de búsqueda para Datapacks — implementa el trait genérico.
pub struct DatapackSearchProvider;

impl ContentSearchProvider for DatapackSearchProvider {
    fn content_type(&self) -> ContentType {
        return ContentType::Datapack;
    }

    fn search(&self, req: &SearchRequest) -> Vec<UnifiedSearchResult> {
        let mut results = Vec::new();
        let extra_facets = vec!["[\"project_type:datapack\"]".to_string()];

        // 1. Modrinth Search (datapacks no dependen del loader)
        let modrinth_hits = modrinth_api::search_modrinth_project(
            &req.query, &None, &req.version, req.offset, req.limit, &extra_facets,
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
                content_type: ContentType::Datapack,
            });
        }

        // 2. CurseForge Search (classId 6945 = Datapacks)
        let cf_key = crate::fetch::cf_api_key();
        if !cf_key.is_empty() {
            let curse_hits = curseforge_api::search_curseforge(
                &req.query, &cf_key, &None, &req.version, req.offset, req.limit, Some(6945),
            );
            for hit in curse_hits {
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
                        content_type: ContentType::Datapack,
                    });
                }
            }
        }
        results
    }

    fn supports_loader_filter(&self) -> bool { false }
    fn supports_version_filter(&self) -> bool { true }
}
