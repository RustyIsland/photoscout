use crate::model::ScanStats;
use serde_json::json;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static DIAGNOSTICS_MODE: OnceLock<DiagnosticsMode> = OnceLock::new();
static BENCH_LOGGER: OnceLock<Mutex<BenchLogger>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticsMode {
    Off,
    Summary,
    Deep,
}

impl DiagnosticsMode {
    pub fn from_env() -> Self {
        let value = std::env::var("PHOTOSCOUT_DIAGNOSTICS")
            .unwrap_or_else(|_| "off".to_string())
            .trim()
            .to_ascii_lowercase();

        match value.as_str() {
            "1" | "true" | "on" | "summary" => Self::Summary,
            "deep" | "verbose" | "full" => Self::Deep,
            _ => Self::Off,
        }
    }

    fn is_enabled(self) -> bool {
        !matches!(self, Self::Off)
    }

    fn is_deep(self) -> bool {
        matches!(self, Self::Deep)
    }
}

struct BenchLogger {
    jsonl: BufWriter<File>,
    summary: BufWriter<File>,
    run_id: String,
}

pub fn init_from_env() {
    let mode = DiagnosticsMode::from_env();
    let _ = DIAGNOSTICS_MODE.set(mode);

    if mode.is_enabled() {
        init_benchmark_logging();
    } else {
        tracing::debug!("PhotoScout diagnostics disabled; set PHOTOSCOUT_DIAGNOSTICS=summary or deep to enable benchmark logs");
    }
}

pub fn mode() -> DiagnosticsMode {
    *DIAGNOSTICS_MODE.get_or_init(DiagnosticsMode::from_env)
}

pub fn deep_enabled() -> bool {
    mode().is_deep()
}

pub fn init_benchmark_logging() {
    if !mode().is_enabled() {
        return;
    }

    let run_id = unix_timestamp_string();
    let run_dir = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("benchmark")
        .join(format!("run_{run_id}"));

    if let Err(error) = fs::create_dir_all(&run_dir) {
        tracing::warn!(?error, "failed to create benchmark directory");
        return;
    }

    let jsonl_path = run_dir.join("events.jsonl");
    let summary_path = run_dir.join("summary.txt");

    let jsonl = match OpenOptions::new().create(true).append(true).open(&jsonl_path) {
        Ok(file) => BufWriter::new(file),
        Err(error) => {
            tracing::warn!(?error, "failed to open benchmark JSONL log");
            return;
        }
    };

    let summary = match OpenOptions::new().create(true).append(true).open(&summary_path) {
        Ok(file) => BufWriter::new(file),
        Err(error) => {
            tracing::warn!(?error, "failed to open benchmark summary log");
            return;
        }
    };

    let logger = BenchLogger {
        jsonl,
        summary,
        run_id: run_id.clone(),
    };

    if BENCH_LOGGER.set(Mutex::new(logger)).is_err() {
        tracing::warn!("benchmark logger was already initialized");
        return;
    }

    write_event(
        "benchmark_logger_started",
        json!({
            "run_id": run_id,
            "benchmark_dir": run_dir.display().to_string(),
            "mode": mode_label(mode()),
            "privacy": "paths and filenames are intentionally not logged; image references use PhotoId and generic metadata only",
            "tags": ["startup", "benchmark", "privacy_safe"],
        }),
    );

    tracing::info!(
        target: "photoscout::bench",
        benchmark_dir = %run_dir.display(),
        mode = mode_label(mode()),
        "PHOTOSCOUT_BENCH_LOG_FILE"
    );
}

pub fn log_runtime_startup() {
    if !mode().is_enabled() {
        return;
    }

    let exe_size = std::env::current_exe()
        .ok()
        .and_then(|path| std::fs::metadata(path).ok())
        .map(|metadata| metadata.len())
        .unwrap_or(0);

    let payload = json!({
        "build_profile": build_profile(),
        "executable_size_mb": round_mb(exe_size),
        "workers_hint": std::thread::available_parallelism().map(|v| v.get()).unwrap_or(0),
        "diagnostics_mode": mode_label(mode()),
        "tags": ["startup", "runtime"],
    });
    write_event("startup", payload.clone());

    tracing::info!(
        target: "photoscout::bench",
        event = "startup",
        build_profile = build_profile(),
        executable_size_mb = round_mb(exe_size),
        diagnostics_mode = mode_label(mode()),
        "PHOTOSCOUT_BENCH_STARTUP"
    );
}

pub fn log_thumbnail_runtime_config(config: ThumbnailRuntimeConfig) {
    if !mode().is_enabled() {
        return;
    }

    write_event(
        "thumbnail_runtime_config",
        json!({
            "preview_workers": config.preview_workers,
            "final_workers": config.final_workers,
            "preview_edge": config.preview_edge,
            "final_edge": config.final_edge,
            "max_visible_preview_requests_per_frame": config.max_visible_preview_requests_per_frame,
            "max_final_requests_per_frame": config.max_final_requests_per_frame,
            "max_prefetch_preview_requests_per_frame": config.max_prefetch_preview_requests_per_frame,
            "max_preview_uploads_per_frame": config.max_preview_uploads_per_frame,
            "max_final_uploads_per_frame": config.max_final_uploads_per_frame,
            "tags": ["thumbnail", "config", "scheduler"],
        }),
    );
}

pub fn log_thumbnail_generation_reset(generation_id: u64, reason: &str) {
    if !mode().is_enabled() {
        return;
    }

    write_event(
        "thumbnail_generation_reset",
        json!({
            "generation_id": generation_id,
            "reason": reason,
            "tags": ["thumbnail", "generation", "reset"],
        }),
    );
}

pub fn log_thumbnail_stale_drop(report: ThumbnailStaleDropReport<'_>) {
    if !mode().is_deep() {
        return;
    }

    write_event(
        "thumbnail_stale_drop",
        json!({
            "generation_id": report.generation_id,
            "current_generation_id": report.current_generation_id,
            "photo_id": report.photo_id,
            "stage": report.stage,
            "priority": report.priority,
            "request_order": report.request_order,
            "complete_order": report.complete_order,
            "worker_pool": report.worker_pool,
            "worker_id": report.worker_id,
            "reason": report.reason,
            "tags": ["thumbnail", "stale", "drop"],
        }),
    );
}

pub fn log_scan_report(total_images: usize, failures: usize, root_count: usize, stats: ScanStats) {
    if !mode().is_enabled() {
        return;
    }

    let total_ms = stats.walk_ms + stats.record_build_ms;
    let files_per_second = per_second(stats.discovered_files, total_ms);
    let kept_per_second = per_second(total_images, total_ms);
    let mb_scanned = bytes_to_mb(stats.candidate_bytes);
    let hash_mb = bytes_to_mb(stats.hash_candidate_bytes);
    let hash_mb_per_second = if stats.hash_ms > 0 {
        (hash_mb * 1000.0) / stats.hash_ms as f64
    } else {
        0.0
    };

    let payload = json!({
        "roots": root_count,
        "discovered_files": stats.discovered_files,
        "kept_images": total_images,
        "failures": failures,
        "skipped_by_file_size": stats.skipped_by_file_size,
        "skipped_by_dimensions": stats.skipped_by_dimensions,
        "candidate_mb": round2(mb_scanned),
        "hash_candidate_files": stats.hash_candidate_files,
        "hash_candidate_mb": round2(hash_mb),
        "walk_ms": stats.walk_ms,
        "record_build_ms": stats.record_build_ms,
        "hash_ms": stats.hash_ms,
        "total_ms": total_ms,
        "discovered_files_per_sec": round2(files_per_second),
        "kept_images_per_sec": round2(kept_per_second),
        "hash_mb_per_sec": round2(hash_mb_per_second),
        "tags": ["scan", "complete"],
    });
    write_event("scan_complete", payload);

    tracing::info!(
        target: "photoscout::bench",
        event = "scan_complete",
        roots = root_count,
        discovered_files = stats.discovered_files,
        kept_images = total_images,
        failures = failures,
        skipped_by_file_size = stats.skipped_by_file_size,
        skipped_by_dimensions = stats.skipped_by_dimensions,
        candidate_mb = round2(mb_scanned),
        hash_candidate_files = stats.hash_candidate_files,
        hash_candidate_mb = round2(hash_mb),
        walk_ms = stats.walk_ms,
        record_build_ms = stats.record_build_ms,
        hash_ms = stats.hash_ms,
        total_ms = total_ms,
        discovered_files_per_sec = round2(files_per_second),
        kept_images_per_sec = round2(kept_per_second),
        hash_mb_per_sec = round2(hash_mb_per_second),
        "PHOTOSCOUT_BENCH_SCAN"
    );
}

pub fn log_thumbnail_report(report: ThumbnailBenchReport) {
    if !mode().is_enabled() {
        return;
    }

    let payload = json!({
        "generation_id": report.generation_id,
        "preview_completed": report.preview_completed,
        "final_completed": report.final_completed,
        "preview_failures": report.preview_failures,
        "final_failures": report.final_failures,
        "preview_avg_decode_ms": round2(report.preview_avg_decode_ms),
        "preview_avg_resize_ms": round2(report.preview_avg_resize_ms),
        "final_avg_decode_ms": round2(report.final_avg_decode_ms),
        "final_avg_resize_ms": round2(report.final_avg_resize_ms),
        "preview_per_sec": round2(report.preview_per_sec),
        "final_per_sec": round2(report.final_per_sec),
        "preview_uploaded": report.preview_uploaded,
        "final_uploaded": report.final_uploaded,
        "deferred_results": report.deferred_results,
        "pending_previews": report.pending_previews,
        "pending_finals": report.pending_finals,
        "tags": ["thumbnail", "window", "throughput"],
    });
    write_event("thumbnail_window", payload);

    tracing::info!(
        target: "photoscout::bench",
        event = "thumbnail_window",
        generation_id = report.generation_id,
        preview_completed = report.preview_completed,
        final_completed = report.final_completed,
        preview_failures = report.preview_failures,
        final_failures = report.final_failures,
        preview_avg_decode_ms = round2(report.preview_avg_decode_ms),
        preview_avg_resize_ms = round2(report.preview_avg_resize_ms),
        final_avg_decode_ms = round2(report.final_avg_decode_ms),
        final_avg_resize_ms = round2(report.final_avg_resize_ms),
        preview_per_sec = round2(report.preview_per_sec),
        final_per_sec = round2(report.final_per_sec),
        preview_uploaded = report.preview_uploaded,
        final_uploaded = report.final_uploaded,
        deferred_results = report.deferred_results,
        pending_previews = report.pending_previews,
        pending_finals = report.pending_finals,
        "PHOTOSCOUT_BENCH_THUMBNAILS"
    );
}

#[derive(Debug, Clone)]
pub struct ThumbnailFileMeta<'a> {
    pub generation_id: u64,
    pub photo_id: u64,
    pub stage: &'a str,
    pub priority: &'a str,
    pub queue_kind: &'a str,
    pub extension: &'a str,
    pub expected_decoder: &'a str,
    pub decode_purpose: &'a str,
    pub size_bytes: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

pub fn log_thumbnail_queued(meta: ThumbnailFileMeta<'_>, request_order: u64) {
    if !mode().is_deep() {
        return;
    }

    write_event(
        "thumbnail_queued",
        json!({
            "generation_id": meta.generation_id,
            "request_order": request_order,
            "photo_id": meta.photo_id,
            "stage": meta.stage,
            "priority": meta.priority,
            "queue_kind": meta.queue_kind,
            "extension": meta.extension,
            "expected_decoder": meta.expected_decoder,
            "decode_purpose": meta.decode_purpose,
            "source_size_kb": round2(meta.size_bytes as f64 / 1024.0),
            "source_width": meta.width,
            "source_height": meta.height,
            "tags": ["thumbnail", "queued", meta.stage, meta.priority, meta.expected_decoder],
        }),
    );
}

#[derive(Debug, Clone)]
pub struct ThumbnailProcessReport<'a> {
    pub generation_id: u64,
    pub request_order: u64,
    pub complete_order: u64,
    pub worker_id: usize,
    pub worker_pool: &'a str,
    pub photo_id: u64,
    pub stage: &'a str,
    pub priority: &'a str,
    pub extension: &'a str,
    pub decoder_kind: &'a str,
    pub decode_purpose: &'a str,
    pub fallback_used: bool,
    pub source_size_bytes: u64,
    pub source_width: Option<u32>,
    pub source_height: Option<u32>,
    pub output_width: Option<usize>,
    pub output_height: Option<usize>,
    pub decode_ms: u128,
    pub resize_ms: u128,
    pub total_ms: u128,
    pub success: bool,
    pub error_kind: Option<&'a str>,
}

pub fn log_thumbnail_processed(report: ThumbnailProcessReport<'_>) {
    if !mode().is_deep() {
        return;
    }

    write_event(
        "thumbnail_processed",
        json!({
            "generation_id": report.generation_id,
            "request_order": report.request_order,
            "complete_order": report.complete_order,
            "worker_id": report.worker_id,
            "worker_pool": report.worker_pool,
            "photo_id": report.photo_id,
            "stage": report.stage,
            "priority": report.priority,
            "extension": report.extension,
            "decoder_kind": report.decoder_kind,
            "decode_purpose": report.decode_purpose,
            "fallback_used": report.fallback_used,
            "source_size_kb": round2(report.source_size_bytes as f64 / 1024.0),
            "source_width": report.source_width,
            "source_height": report.source_height,
            "output_width": report.output_width,
            "output_height": report.output_height,
            "decode_ms": report.decode_ms,
            "resize_ms": report.resize_ms,
            "total_ms": report.total_ms,
            "success": report.success,
            "error_kind": report.error_kind,
            "tags": ["thumbnail", "processed", report.stage, report.worker_pool, report.decoder_kind],
        }),
    );
}

pub fn log_thumbnail_uploaded(
    generation_id: u64,
    photo_id: u64,
    stage: &str,
    upload_order: u64,
    request_order: u64,
    complete_order: u64,
    texture_width: usize,
    texture_height: usize,
) {
    if !mode().is_deep() {
        return;
    }

    write_event(
        "thumbnail_uploaded",
        json!({
            "generation_id": generation_id,
            "upload_order": upload_order,
            "request_order": request_order,
            "complete_order": complete_order,
            "photo_id": photo_id,
            "stage": stage,
            "texture_width": texture_width,
            "texture_height": texture_height,
            "tags": ["thumbnail", "uploaded", stage],
        }),
    );
}

#[derive(Debug, Clone, Copy)]
pub struct ThumbnailBenchReport {
    pub generation_id: u64,
    pub preview_completed: usize,
    pub final_completed: usize,
    pub preview_failures: usize,
    pub final_failures: usize,
    pub preview_avg_decode_ms: f64,
    pub preview_avg_resize_ms: f64,
    pub final_avg_decode_ms: f64,
    pub final_avg_resize_ms: f64,
    pub preview_per_sec: f64,
    pub final_per_sec: f64,
    pub preview_uploaded: usize,
    pub final_uploaded: usize,
    pub deferred_results: usize,
    pub pending_previews: usize,
    pub pending_finals: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct ThumbnailRuntimeConfig {
    pub preview_workers: usize,
    pub final_workers: usize,
    pub preview_edge: u32,
    pub final_edge: u32,
    pub max_visible_preview_requests_per_frame: usize,
    pub max_final_requests_per_frame: usize,
    pub max_prefetch_preview_requests_per_frame: usize,
    pub max_preview_uploads_per_frame: usize,
    pub max_final_uploads_per_frame: usize,
}

#[derive(Debug, Clone)]
pub struct ThumbnailStaleDropReport<'a> {
    pub generation_id: u64,
    pub current_generation_id: u64,
    pub photo_id: u64,
    pub stage: &'a str,
    pub priority: &'a str,
    pub request_order: u64,
    pub complete_order: u64,
    pub worker_pool: &'a str,
    pub worker_id: usize,
    pub reason: &'a str,
}

fn write_event(event: &str, payload: serde_json::Value) {
    let Some(logger) = BENCH_LOGGER.get() else {
        return;
    };

    let Ok(mut logger) = logger.lock() else {
        return;
    };

    let timestamp_ms = unix_timestamp_ms();
    let record = json!({
        "timestamp_ms": timestamp_ms,
        "run_id": logger.run_id.clone(),
        "event": event,
        "payload": payload,
    });

    if let Err(error) = writeln!(logger.jsonl, "{record}") {
        tracing::warn!(?error, "failed to write benchmark JSONL event");
    }
    let _ = logger.jsonl.flush();

    let should_write_summary = match mode() {
        DiagnosticsMode::Off => false,
        DiagnosticsMode::Summary => !matches!(
            event,
            "thumbnail_processed" | "thumbnail_queued" | "thumbnail_uploaded" | "thumbnail_stale_drop"
        ),
        DiagnosticsMode::Deep => !matches!(
            event,
            "thumbnail_processed" | "thumbnail_queued" | "thumbnail_uploaded"
        ),
    };

    if should_write_summary {
        let _ = writeln!(logger.summary, "{timestamp_ms} {event}: {record}");
        let _ = logger.summary.flush();
    }
}

fn mode_label(mode: DiagnosticsMode) -> &'static str {
    match mode {
        DiagnosticsMode::Off => "off",
        DiagnosticsMode::Summary => "summary",
        DiagnosticsMode::Deep => "deep",
    }
}

fn build_profile() -> &'static str {
    if cfg!(debug_assertions) { "debug" } else { "release" }
}

fn unix_timestamp_string() -> String {
    unix_timestamp_ms().to_string()
}

fn unix_timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn per_second(count: usize, millis: u128) -> f64 {
    if millis == 0 { 0.0 } else { (count as f64 * 1000.0) / millis as f64 }
}

fn bytes_to_mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn round_mb(bytes: u64) -> f64 {
    round2(bytes_to_mb(bytes))
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

pub fn millis(duration: Duration) -> u128 {
    duration.as_millis()
}
