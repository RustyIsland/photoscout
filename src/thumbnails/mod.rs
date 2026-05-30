mod bench;
mod resize;
mod types;
mod workers;

use self::bench::ThumbnailBenchAccumulator;
use self::types::{TextureEntry, ThumbnailPriority, ThumbnailRequest, ThumbnailResult, ThumbnailStage};
use self::workers::{decode_purpose_for, expected_decoder_for, final_worker_loop, preview_worker_loop};
use crate::diagnostics::{self, ThumbnailFileMeta, ThumbnailRuntimeConfig, ThumbnailStaleDropReport};
use crate::model::{PhotoId, PhotoRecord};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use eframe::egui;
use std::collections::{HashMap, HashSet, VecDeque};
use std::thread;
use std::time::Duration;

pub use self::types::ThumbnailRender;

const THUMBNAIL_EDGE: u32 = 176;
const PREVIEW_EDGE: u32 = 96;
const MAX_TOTAL_THUMBNAIL_WORKERS: usize = 4;
const MAX_VISIBLE_PREVIEW_REQUESTS_PER_FRAME: usize = 64;
const MAX_FINAL_REQUESTS_PER_FRAME: usize = 8;
const MAX_PREFETCH_PREVIEW_REQUESTS_PER_FRAME: usize = 12;
const HIGH_PRIORITY_QUEUE_BOUND: usize = 1024;
const NORMAL_PRIORITY_QUEUE_BOUND: usize = 2048;
const DEFAULT_MAX_PREVIEW_UPLOADS_PER_FRAME: usize = 48;
const DEFAULT_MAX_FINAL_UPLOADS_PER_FRAME: usize = 6;
const MAX_TEXTURE_CACHE_ENTRIES: usize = 900;
const TEXTURE_EVICTION_BATCH: usize = 120;

pub struct ThumbnailCache {
    textures: HashMap<PhotoId, TextureEntry>,
    texture_last_used: HashMap<PhotoId, u64>,
    texture_clock: u64,
    pending_preview: HashSet<PhotoId>,
    pending_final: HashSet<PhotoId>,
    failures: HashMap<PhotoId, String>,
    visible_preview_tx: Sender<ThumbnailRequest>,
    prefetch_preview_tx: Sender<ThumbnailRequest>,
    final_request_tx: Sender<ThumbnailRequest>,
    result_rx: Receiver<ThumbnailResult>,
    deferred_results: VecDeque<ThumbnailResult>,
    visible_preview_requests_this_frame: usize,
    final_requests_this_frame: usize,
    prefetch_preview_requests_this_frame: usize,
    next_request_order: u64,
    next_upload_order: u64,
    generation_id: u64,
    bench: ThumbnailBenchAccumulator,
}

impl Default for ThumbnailCache {
    fn default() -> Self {
        let (visible_preview_tx, visible_preview_rx) = bounded::<ThumbnailRequest>(HIGH_PRIORITY_QUEUE_BOUND);
        let (prefetch_preview_tx, prefetch_preview_rx) = bounded::<ThumbnailRequest>(NORMAL_PRIORITY_QUEUE_BOUND);
        let (final_request_tx, final_request_rx) = bounded::<ThumbnailRequest>(NORMAL_PRIORITY_QUEUE_BOUND);
        let (result_tx, result_rx) = unbounded::<ThumbnailResult>();

        let total_workers = std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(4)
            .clamp(2, MAX_TOTAL_THUMBNAIL_WORKERS.max(2));
        let preview_workers = total_workers.saturating_sub(1).max(1);
        let final_workers = (total_workers - preview_workers).max(1);

        for worker_id in 0..preview_workers {
            let visible_preview_rx = visible_preview_rx.clone();
            let prefetch_preview_rx = prefetch_preview_rx.clone();
            let result_tx = result_tx.clone();
            thread::Builder::new()
                .name(format!("photoscout-preview-{worker_id}"))
                .spawn(move || {
                    preview_worker_loop(
                        worker_id,
                        visible_preview_rx,
                        prefetch_preview_rx,
                        result_tx,
                        PREVIEW_EDGE,
                        THUMBNAIL_EDGE,
                    );
                })
                .expect("failed to start preview thumbnail worker");
        }

        for worker_id in 0..final_workers {
            let final_request_rx = final_request_rx.clone();
            let result_tx = result_tx.clone();
            thread::Builder::new()
                .name(format!("photoscout-final-{worker_id}"))
                .spawn(move || {
                    final_worker_loop(
                        worker_id,
                        final_request_rx,
                        result_tx,
                        PREVIEW_EDGE,
                        THUMBNAIL_EDGE,
                    );
                })
                .expect("failed to start final thumbnail worker");
        }

        diagnostics::log_thumbnail_runtime_config(ThumbnailRuntimeConfig {
            preview_workers,
            final_workers,
            preview_edge: PREVIEW_EDGE,
            final_edge: THUMBNAIL_EDGE,
            max_visible_preview_requests_per_frame: MAX_VISIBLE_PREVIEW_REQUESTS_PER_FRAME,
            max_final_requests_per_frame: MAX_FINAL_REQUESTS_PER_FRAME,
            max_prefetch_preview_requests_per_frame: MAX_PREFETCH_PREVIEW_REQUESTS_PER_FRAME,
            max_preview_uploads_per_frame: DEFAULT_MAX_PREVIEW_UPLOADS_PER_FRAME,
            max_final_uploads_per_frame: DEFAULT_MAX_FINAL_UPLOADS_PER_FRAME,
        });

        Self {
            textures: HashMap::new(),
            texture_last_used: HashMap::new(),
            texture_clock: 0,
            pending_preview: HashSet::new(),
            pending_final: HashSet::new(),
            failures: HashMap::new(),
            visible_preview_tx,
            prefetch_preview_tx,
            final_request_tx,
            result_rx,
            deferred_results: VecDeque::new(),
            visible_preview_requests_this_frame: 0,
            final_requests_this_frame: 0,
            prefetch_preview_requests_this_frame: 0,
            next_request_order: 1,
            next_upload_order: 1,
            generation_id: 1,
            bench: ThumbnailBenchAccumulator::for_generation(1),
        }
    }
}

impl ThumbnailCache {
    pub fn begin_frame(&mut self) {
        self.visible_preview_requests_this_frame = 0;
        self.final_requests_this_frame = 0;
        self.prefetch_preview_requests_this_frame = 0;
    }

    pub fn clear(&mut self) {
        self.generation_id = self.generation_id.saturating_add(1);
        diagnostics::log_thumbnail_generation_reset(self.generation_id, "thumbnail_cache_clear");
        self.textures.clear();
        self.texture_last_used.clear();
        self.texture_clock = 0;
        self.pending_preview.clear();
        self.pending_final.clear();
        self.failures.clear();
        self.deferred_results.clear();
        self.visible_preview_requests_this_frame = 0;
        self.final_requests_this_frame = 0;
        self.prefetch_preview_requests_this_frame = 0;
        self.next_request_order = 1;
        self.next_upload_order = 1;
        self.bench = ThumbnailBenchAccumulator::for_generation(self.generation_id);

        while self.result_rx.try_recv().is_ok() {}
    }

    pub fn drain_completed(
        &mut self,
        ctx: &egui::Context,
        max_preview_uploads: usize,
        max_final_uploads: usize,
    ) {
        for _ in 0..512 {
            let Ok(result) = self.result_rx.try_recv() else {
                break;
            };

            if result.generation_id != self.generation_id {
                diagnostics::log_thumbnail_stale_drop(ThumbnailStaleDropReport {
                    generation_id: result.generation_id,
                    current_generation_id: self.generation_id,
                    photo_id: result.id.0,
                    stage: result.stage.label(),
                    priority: result.priority.label(),
                    request_order: result.request_order,
                    complete_order: result.complete_order,
                    worker_pool: result.worker_pool.label(),
                    worker_id: result.worker_id,
                    reason: "result_from_previous_generation",
                });
                continue;
            }

            self.bench.record_result(&result);
            self.deferred_results.push_back(result);
        }

        let mut preview_uploads = 0usize;
        let mut final_uploads = 0usize;
        let mut kept_for_later = VecDeque::new();

        while let Some(result) = self.deferred_results.pop_front() {
            let can_upload = match result.stage {
                ThumbnailStage::Preview => preview_uploads < max_preview_uploads,
                ThumbnailStage::Final => final_uploads < max_final_uploads,
            };

            if !can_upload {
                kept_for_later.push_back(result);
                continue;
            }

            match result.result {
                Ok(color_image) => {
                    self.touch_texture(result.id);
                    let entry = self.textures.entry(result.id).or_default();

                    if result.stage == ThumbnailStage::Preview && entry.final_texture.is_some() {
                        self.pending_preview.remove(&result.id);
                        continue;
                    }

                    let texture_size = color_image.size;
                    let texture = ctx.load_texture(
                        format!(
                            "photo-thumb-gen{}-{}-{:?}",
                            self.generation_id, result.id.0, result.stage
                        ),
                        color_image,
                        egui::TextureOptions::LINEAR,
                    );

                    let upload_order = self.next_upload_order;
                    self.next_upload_order = self.next_upload_order.saturating_add(1);
                    if diagnostics::deep_enabled() {
                        diagnostics::log_thumbnail_uploaded(
                            self.generation_id,
                            result.id.0,
                            result.stage.label(),
                            upload_order,
                            result.request_order,
                            result.complete_order,
                            texture_size[0],
                            texture_size[1],
                        );
                    }

                    match result.stage {
                        ThumbnailStage::Preview => {
                            entry.preview = Some(texture);
                            self.pending_preview.remove(&result.id);
                            preview_uploads += 1;
                            self.bench.record_upload(ThumbnailStage::Preview);
                        }
                        ThumbnailStage::Final => {
                            entry.final_texture = Some(texture);
                            self.pending_final.remove(&result.id);
                            final_uploads += 1;
                            self.bench.record_upload(ThumbnailStage::Final);
                        }
                    }
                }
                Err(error) => {
                    match result.stage {
                        ThumbnailStage::Preview => {
                            self.pending_preview.remove(&result.id);
                        }
                        ThumbnailStage::Final => {
                            self.pending_final.remove(&result.id);
                        }
                    }
                    self.failures.insert(result.id, error);
                }
            }
        }

        self.deferred_results = kept_for_later;
        self.evict_old_textures_if_needed();

        self.bench.maybe_report(
            self.deferred_results.len(),
            self.pending_preview.len(),
            self.pending_final.len(),
        );

        if preview_uploads > 0
            || final_uploads > 0
            || !self.pending_preview.is_empty()
            || !self.pending_final.is_empty()
            || !self.deferred_results.is_empty()
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    pub fn texture_for_visible(
        &mut self,
        ctx: &egui::Context,
        photo: &PhotoRecord,
    ) -> Option<ThumbnailRender<'_>> {
        self.touch_texture(photo.id);

        if !self.has_preview_texture(photo.id)
            && !self.has_final_texture(photo.id)
            && !self.failures.contains_key(&photo.id)
        {
            self.queue_request(photo, ThumbnailPriority::VisiblePreview);
            ctx.request_repaint_after(Duration::from_millis(16));
        }

        self.evict_old_textures_if_needed();

        self.textures.get(&photo.id).and_then(|entry| {
            if let Some(texture) = entry.final_texture.as_ref() {
                Some(ThumbnailRender {
                    texture,
                    is_preview: false,
                })
            } else {
                entry.preview.as_ref().map(|texture| ThumbnailRender {
                    texture,
                    is_preview: true,
                })
            }
        })
    }

    pub fn visible_previews_ready<'a>(&self, photos: impl IntoIterator<Item = &'a PhotoRecord>) -> bool {
        let mut checked = 0usize;

        for photo in photos {
            checked += 1;
            if self.failures.contains_key(&photo.id) {
                continue;
            }
            if self.has_final_texture(photo.id) {
                continue;
            }
            if self.has_preview_texture(photo.id) && !self.pending_preview.contains(&photo.id) {
                continue;
            }
            return false;
        }

        checked > 0
    }

    pub fn refine_visible_thumbnails<'a>(&mut self, photos: impl IntoIterator<Item = &'a PhotoRecord>) {
        if !self.pending_preview.is_empty() {
            return;
        }

        let mut seen = HashSet::new();
        for photo in photos {
            if !seen.insert(photo.id) {
                continue;
            }
            if self.final_requests_this_frame >= MAX_FINAL_REQUESTS_PER_FRAME {
                break;
            }
            if !self.has_preview_texture(photo.id)
                || self.has_final_texture(photo.id)
                || self.failures.contains_key(&photo.id)
            {
                continue;
            }
            self.queue_request(photo, ThumbnailPriority::FinalRefine);
        }
    }

    pub fn prefetch_visible_neighbors<'a>(&mut self, photos: impl IntoIterator<Item = &'a PhotoRecord>) {
        let mut seen = HashSet::new();
        for photo in photos {
            if !seen.insert(photo.id) {
                continue;
            }
            if self.prefetch_preview_requests_this_frame >= MAX_PREFETCH_PREVIEW_REQUESTS_PER_FRAME {
                break;
            }
            if self.has_preview_texture(photo.id)
                || self.has_final_texture(photo.id)
                || self.failures.contains_key(&photo.id)
            {
                continue;
            }
            self.queue_request(photo, ThumbnailPriority::PrefetchPreview);
        }
    }


    pub fn forget_photos(&mut self, ids: &HashSet<PhotoId>) {
        for id in ids {
            self.textures.remove(id);
            self.texture_last_used.remove(id);
            self.pending_preview.remove(id);
            self.pending_final.remove(id);
            self.failures.remove(id);
        }

        self.deferred_results.retain(|result| !ids.contains(&result.id));
    }

    fn touch_texture(&mut self, id: PhotoId) {
        self.texture_clock = self.texture_clock.saturating_add(1);
        self.texture_last_used.insert(id, self.texture_clock);
    }

    fn evict_old_textures_if_needed(&mut self) {
        if self.textures.len() <= MAX_TEXTURE_CACHE_ENTRIES {
            return;
        }

        let mut candidates: Vec<(PhotoId, u64)> = self
            .textures
            .keys()
            .filter(|id| !self.pending_preview.contains(id) && !self.pending_final.contains(id))
            .map(|id| (*id, self.texture_last_used.get(id).copied().unwrap_or(0)))
            .collect();

        candidates.sort_by_key(|(_, last_used)| *last_used);

        let remove_count = self
            .textures
            .len()
            .saturating_sub(MAX_TEXTURE_CACHE_ENTRIES)
            .saturating_add(TEXTURE_EVICTION_BATCH)
            .min(candidates.len());

        for (id, _) in candidates.into_iter().take(remove_count) {
            self.textures.remove(&id);
            self.texture_last_used.remove(&id);
        }
    }

    fn has_preview_texture(&self, id: PhotoId) -> bool {
        self.textures
            .get(&id)
            .and_then(|entry| entry.preview.as_ref())
            .is_some()
    }

    fn has_final_texture(&self, id: PhotoId) -> bool {
        self.textures
            .get(&id)
            .and_then(|entry| entry.final_texture.as_ref())
            .is_some()
    }

    fn queue_request(&mut self, photo: &PhotoRecord, priority: ThumbnailPriority) {
        match priority {
            ThumbnailPriority::VisiblePreview => {
                if self.visible_preview_requests_this_frame >= MAX_VISIBLE_PREVIEW_REQUESTS_PER_FRAME {
                    return;
                }
                if self.has_preview_texture(photo.id)
                    || self.has_final_texture(photo.id)
                    || self.pending_preview.contains(&photo.id)
                {
                    return;
                }
            }
            ThumbnailPriority::FinalRefine => {
                if self.final_requests_this_frame >= MAX_FINAL_REQUESTS_PER_FRAME {
                    return;
                }
                if !self.has_preview_texture(photo.id)
                    || self.has_final_texture(photo.id)
                    || self.pending_final.contains(&photo.id)
                    || !self.pending_preview.is_empty()
                {
                    return;
                }
            }
            ThumbnailPriority::PrefetchPreview => {
                if self.prefetch_preview_requests_this_frame >= MAX_PREFETCH_PREVIEW_REQUESTS_PER_FRAME {
                    return;
                }
                if self.has_preview_texture(photo.id)
                    || self.has_final_texture(photo.id)
                    || self.pending_preview.contains(&photo.id)
                {
                    return;
                }
            }
        }

        let request_order = self.next_request_order;
        self.next_request_order = self.next_request_order.saturating_add(1);
        let stage = match priority {
            ThumbnailPriority::VisiblePreview | ThumbnailPriority::PrefetchPreview => ThumbnailStage::Preview,
            ThumbnailPriority::FinalRefine => ThumbnailStage::Final,
        };

        let request = ThumbnailRequest {
            generation_id: self.generation_id,
            id: photo.id,
            path: photo.path.clone(),
            stage,
            priority,
            request_order,
            extension: photo.extension.clone(),
            size_bytes: photo.size_bytes,
            source_width: photo.width,
            source_height: photo.height,
        };

        let sent = match priority {
            ThumbnailPriority::VisiblePreview => {
                self.visible_preview_requests_this_frame += 1;
                self.visible_preview_tx.try_send(request).is_ok()
            }
            ThumbnailPriority::FinalRefine => {
                self.final_requests_this_frame += 1;
                self.final_request_tx.try_send(request).is_ok()
            }
            ThumbnailPriority::PrefetchPreview => {
                self.prefetch_preview_requests_this_frame += 1;
                self.prefetch_preview_tx.try_send(request).is_ok()
            }
        };

        if sent {
            if diagnostics::deep_enabled() {
                diagnostics::log_thumbnail_queued(
                    ThumbnailFileMeta {
                        generation_id: self.generation_id,
                        photo_id: photo.id.0,
                        stage: stage.label(),
                        priority: priority.label(),
                        queue_kind: priority.queue_kind(),
                        extension: &photo.extension,
                        expected_decoder: expected_decoder_for(&photo.extension).label(),
                        decode_purpose: decode_purpose_for(stage).label(),
                        size_bytes: photo.size_bytes,
                        width: photo.width,
                        height: photo.height,
                    },
                    request_order,
                );
            }

            match priority {
                ThumbnailPriority::VisiblePreview | ThumbnailPriority::PrefetchPreview => {
                    self.pending_preview.insert(photo.id);
                }
                ThumbnailPriority::FinalRefine => {
                    self.pending_final.insert(photo.id);
                }
            }
        }
    }
}
