use std::ops::{Deref, DerefMut};
use crate::local_mods_ops::ModInfo;
use crate::fetch::single_mod_search::UnifiedSearchResult;

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum SearchSource {
    Explorer,
    Profile(String),
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
