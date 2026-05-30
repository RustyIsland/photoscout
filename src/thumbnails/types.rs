use crate::model::PhotoId;
use eframe::egui;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ThumbnailPriority {
    VisiblePreview,
    FinalRefine,
    PrefetchPreview,
}

impl ThumbnailPriority {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::VisiblePreview => "visible_preview",
            Self::FinalRefine => "final_refine",
            Self::PrefetchPreview => "prefetch_preview",
        }
    }

    pub(super) fn queue_kind(self) -> &'static str {
        match self {
            Self::VisiblePreview => "visible_preview_queue",
            Self::FinalRefine => "final_queue",
            Self::PrefetchPreview => "prefetch_preview_queue",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ThumbnailStage {
    Preview,
    Final,
}

impl ThumbnailStage {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Preview => "ugly_preview",
            Self::Final => "pretty_final",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkerPool {
    Preview,
    Final,
}

impl WorkerPool {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Preview => "preview_pool",
            Self::Final => "final_pool",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ThumbnailRequest {
    pub(super) generation_id: u64,
    pub(super) id: PhotoId,
    pub(super) path: PathBuf,
    pub(super) stage: ThumbnailStage,
    pub(super) priority: ThumbnailPriority,
    pub(super) request_order: u64,
    pub(super) extension: String,
    pub(super) size_bytes: u64,
    pub(super) source_width: Option<u32>,
    pub(super) source_height: Option<u32>,
}

#[derive(Debug)]
pub(super) struct ThumbnailResult {
    pub(super) generation_id: u64,
    pub(super) id: PhotoId,
    pub(super) stage: ThumbnailStage,
    pub(super) priority: ThumbnailPriority,
    pub(super) request_order: u64,
    pub(super) complete_order: u64,
    pub(super) worker_id: usize,
    pub(super) worker_pool: WorkerPool,
    pub(super) result: Result<egui::ColorImage, String>,
    pub(super) decode_ms: u128,
    pub(super) resize_ms: u128,
}

#[derive(Default)]
pub(super) struct TextureEntry {
    pub(super) preview: Option<egui::TextureHandle>,
    pub(super) final_texture: Option<egui::TextureHandle>,
}

pub struct ThumbnailRender<'a> {
    pub texture: &'a egui::TextureHandle,
    pub is_preview: bool,
}
