use std::ops::{Deref, DerefMut};
use crate::local_mods_ops::ModInfo;
use crate::fetch::search_provider::{UnifiedSearchResult, ContentType};
use crate::fetch::async_download::DownloadJob;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadAction {
    Install,
    Replace,
    Skip,
}

#[derive(Debug, Clone)]
pub struct DuplicateResolution {
    pub modinfo: ModInfo,
    pub download_job: DownloadJob,
    pub existing_filename: Option<String>,
    pub existing_version: Option<String>,
    pub action: DownloadAction,
    pub status: ModStatus,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum SearchSource {
    Explorer,
    Profile(String),
    World(String),  // Datapack search for a specific world/save
}

pub struct SearchState {
    pub query: String,
    pub loader: String,
    pub version: String,
    pub results: Vec<UnifiedSearchResult>,
    pub is_searching: bool,
    pub open: bool,
    pub source: SearchSource,
    pub page: u32,
    pub limit: u32,
    pub download_dependencies: bool,
    pub content_type: ContentType,
    // Version Selection State
    pub selected_project_for_versions: Option<UnifiedSearchResult>,
    pub project_versions_results: Vec<crate::fetch::search_provider::ProjectVersion>,
    pub is_fetching_versions: bool,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            query: String::new(),
            loader: "Fabric".to_string(), // Default, will be overwritten by app selection
            version: "1.20.1".to_string(),
            results: Vec::new(),
            is_searching: false,
            open: false,
            source: SearchSource::Explorer,
            page: 0,
            limit: 10,
            download_dependencies: true,
            content_type: ContentType::Mod,
            selected_project_for_versions: None,
            project_versions_results: Vec::new(),
            is_fetching_versions: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ModStatus {
    Idle,
    Resolving,
    Downloading(f32), // progress 0.0 - 1.0
    Done,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct UiModInfo {
    pub inner: ModInfo,
    pub status: ModStatus,
    pub progress: f32,
}

impl From<ModInfo> for UiModInfo {
    fn from(m: ModInfo) -> Self {
        UiModInfo { inner: m, status: ModStatus::Idle, progress: 0.0 }
    }
}

impl Deref for UiModInfo {
    type Target = ModInfo;
    fn deref(&self) -> &ModInfo { &self.inner }
}

impl DerefMut for UiModInfo {
    fn deref_mut(&mut self) -> &mut ModInfo { &mut self.inner }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeletionConfirmation {
    None,
    Modpack(String),
    SelectedMods,
    Profile(String), // Confirm deletion of a profile
    Datapack(String, String), // (world_name, filename)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadSource {
    Explorer,
    Profile(String),
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum AppTab {
    Explorer,
    Profiles,
    Datapacks,
}
