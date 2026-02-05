use super::fetch_from_api;
use std::env;

#[derive(Debug, Clone)]
pub struct UnifiedSearchResult {
    pub name: String,
    pub slug: String, // Used for logical ID if needed
    pub description: String,
    pub icon_url: Option<String>,
    pub author: String,
    
    // Source specific info to allow downloading/linking
    pub modrinth_id: Option<String>,
    pub curseforge_id: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub query: String,
    pub loader: Option<String>,
    pub version: Option<String>,
    pub offset: u32,
    pub limit: u32,
}

pub fn search_unified(req: &SearchRequest) -> Vec<UnifiedSearchResult> {
    let mut results = Vec::new();
    
    // 1. Modrinth Search
    let modrinth_hits = fetch_from_api::search_modrinth_project(&req.query, &req.loader, &req.version, req.offset, req.limit);
    for hit in modrinth_hits {
        results.push(UnifiedSearchResult {
            name: hit.title,
            slug: hit.slug,
            description: hit.description.unwrap_or_default(),
            icon_url: hit.icon_url,
            author: hit.author,
            modrinth_id: Some(hit.project_id),
            curseforge_id: None,
        });
    }

    // 2. CurseForge Search
    let cf_key = env::var("CURSEFORGE_API_KEY").unwrap_or_default();
    if !cf_key.is_empty() {
        let curse_hits = fetch_from_api::search_curseforge(&req.query, &cf_key, &req.loader, &req.version, req.offset, req.limit);
        for hit in curse_hits {
            // Check for duplicates (primitive dedup by name or slug)
            if let Some(existing) = results.iter_mut().find(|r| r.slug == hit.slug || r.name == hit.name) {
                // Merge info if needed
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
                    author: "Unknown (CF)".to_string(), // CF search doesn't easily give author in summary, typically in 'authors' array which we didn't fully map yet or trivial to get
                    modrinth_id: None,
                    curseforge_id: Some(hit.id),
                });
            }
        }
    }

    results
}
