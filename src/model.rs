use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LibraryRootId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PhotoId(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryRoot {
    pub id: LibraryRootId,
    pub path: PathBuf,
    pub label: String,
    pub enabled: bool,
}

impl LibraryRoot {
    pub fn new(id: LibraryRootId, path: PathBuf) -> Self {
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| path.display().to_string());

        Self {
            id,
            path,
            label,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoRecord {
    pub id: PhotoId,
    pub root_id: LibraryRootId,
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub file_name: String,
    pub extension: String,
    pub size_bytes: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    NameAsc,
    SizeLargest,
    RootThenName,
}

impl Default for SortMode {
    fn default() -> Self {
        Self::NameAsc
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub text: String,
    pub duplicates_only: bool,
    pub root_filter: Option<LibraryRootId>,
    pub sort_mode: SortMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanOptions {
    pub min_file_size_bytes: u64,
    pub min_width: u32,
    pub min_height: u32,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            min_file_size_bytes: 0,
            min_width: 0,
            min_height: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ScanStats {
    pub discovered_files: usize,
    pub skipped_by_file_size: usize,
    pub skipped_by_dimensions: usize,
    pub candidate_bytes: u64,
    pub hash_candidate_files: usize,
    pub hash_candidate_bytes: u64,
    pub walk_ms: u128,
    pub record_build_ms: u128,
    pub hash_ms: u128,
}
