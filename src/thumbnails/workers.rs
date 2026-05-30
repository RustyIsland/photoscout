use super::resize::{make_final_thumbnail_fast, make_preview_thumbnail};
use super::types::{ThumbnailRequest, ThumbnailResult, ThumbnailStage, WorkerPool};
use crate::diagnostics::{self, ThumbnailProcessReport};
use crate::image_decoders::{decode_for_thumbnail, DecodePurpose, DecoderKind};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static COMPLETE_ORDER: AtomicU64 = AtomicU64::new(1);

pub(super) fn preview_worker_loop(
    worker_id: usize,
    visible_preview_rx: Receiver<ThumbnailRequest>,
    prefetch_preview_rx: Receiver<ThumbnailRequest>,
    result_tx: Sender<ThumbnailResult>,
    preview_edge: u32,
    final_edge: u32,
) {
    loop {
        let request = match visible_preview_rx.try_recv() {
            Ok(request) => request,
            Err(_) => match prefetch_preview_rx.recv_timeout(Duration::from_millis(8)) {
                Ok(request) => request,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            },
        };

        process_thumbnail_request(worker_id, WorkerPool::Preview, request, &result_tx, preview_edge, final_edge);
    }
}

pub(super) fn final_worker_loop(
    worker_id: usize,
    final_request_rx: Receiver<ThumbnailRequest>,
    result_tx: Sender<ThumbnailResult>,
    preview_edge: u32,
    final_edge: u32,
) {
    loop {
        let request = match final_request_rx.recv_timeout(Duration::from_millis(16)) {
            Ok(request) => request,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };

        process_thumbnail_request(worker_id, WorkerPool::Final, request, &result_tx, preview_edge, final_edge);
    }
}

fn process_thumbnail_request(
    worker_id: usize,
    worker_pool: WorkerPool,
    request: ThumbnailRequest,
    result_tx: &Sender<ThumbnailResult>,
    preview_edge: u32,
    final_edge: u32,
) {
    let total_started = Instant::now();
    let decode_purpose = decode_purpose_for(request.stage);
    let decode_started = Instant::now();
    let decoded = decode_for_thumbnail(&request.path, &request.extension, decode_purpose);
    let decode_ms = decode_started.elapsed().as_millis();

    let decoder_kind = decoded
        .as_ref()
        .map(|decoded| decoded.decoder)
        .unwrap_or_else(|_| expected_decoder_for(&request.extension));
    let fallback_used = decoded
        .as_ref()
        .map(|decoded| decoded.fallback_used)
        .unwrap_or(false);

    let resize_started = Instant::now();
    let result = match decoded {
        Ok(source) => match request.stage {
            ThumbnailStage::Preview => make_preview_thumbnail(&source.image, preview_edge),
            ThumbnailStage::Final => make_final_thumbnail_fast(&source.image, final_edge),
        },
        Err(error) => Err(error),
    };
    let resize_ms = if result.is_ok() {
        resize_started.elapsed().as_millis()
    } else {
        0
    };
    let total_ms = total_started.elapsed().as_millis();
    let complete_order = COMPLETE_ORDER.fetch_add(1, Ordering::Relaxed);

    let (output_width, output_height) = result
        .as_ref()
        .map(|image| (Some(image.size[0]), Some(image.size[1])))
        .unwrap_or((None, None));
    let error_kind = result.as_ref().err().map(|_| "decode_or_resize_failed");

    if diagnostics::deep_enabled() {
        diagnostics::log_thumbnail_processed(ThumbnailProcessReport {
            generation_id: request.generation_id,
            request_order: request.request_order,
            complete_order,
            worker_id,
            worker_pool: worker_pool.label(),
            photo_id: request.id.0,
            stage: request.stage.label(),
            priority: request.priority.label(),
            extension: &request.extension,
            decoder_kind: decoder_kind.label(),
            decode_purpose: decode_purpose.label(),
            fallback_used,
            source_size_bytes: request.size_bytes,
            source_width: request.source_width,
            source_height: request.source_height,
            output_width,
            output_height,
            decode_ms,
            resize_ms,
            total_ms,
            success: result.is_ok(),
            error_kind,
        });
    }

    let _ = result_tx.send(ThumbnailResult {
        generation_id: request.generation_id,
        id: request.id,
        stage: request.stage,
        priority: request.priority,
        request_order: request.request_order,
        complete_order,
        worker_id,
        worker_pool,
        result,
        decode_ms,
        resize_ms,
    });
}

pub(super) fn expected_decoder_for(extension: &str) -> DecoderKind {
    match extension.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => DecoderKind::ZuneJpeg,
        _ => DecoderKind::ImageCrateGeneric,
    }
}

pub(super) fn decode_purpose_for(stage: ThumbnailStage) -> DecodePurpose {
    match stage {
        ThumbnailStage::Preview => DecodePurpose::Preview,
        ThumbnailStage::Final => DecodePurpose::Final,
    }
}
