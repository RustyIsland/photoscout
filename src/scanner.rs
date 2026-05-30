use crate::diagnostics::millis;
use crate::error::PhotoScoutError;
use crate::model::{LibraryRoot, PhotoId, PhotoRecord, ScanOptions, ScanStats};
use crate::path_utils::{is_supported_image, lower_extension};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::time::Instant;
use walkdir::WalkDir;

const WALK_PROGRESS_EVERY: usize = 250;
const RECORD_PROGRESS_EVERY: usize = 100;

#[derive(Debug, Clone)]
struct CandidatePhoto {
    root: LibraryRoot,
    path: PathBuf,
    size_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct ScanProgress {
    pub phase: &'static str,
    pub discovered_files: usize,
    pub candidate_files: usize,
    pub processed_candidates: usize,
    pub kept_images: usize,
}

#[derive(Debug)]
pub enum ScanEvent {
    Progress(ScanProgress),
    PhotoFound(PhotoRecord),
    Failed(PhotoScoutError),
}

#[derive(Debug, Clone, Copy)]
pub struct StreamingScanOutput {
    pub total_images: usize,
    pub failures: usize,
    pub stats: ScanStats,
}

pub fn scan_roots_streaming(
    roots: &[LibraryRoot],
    options: ScanOptions,
    event_sender: Sender<ScanEvent>,
) -> StreamingScanOutput {
    let walk_started = Instant::now();
    let mut candidates = Vec::new();
    let mut discovered_files = 0usize;
    let mut skipped_by_file_size = 0usize;
    let mut candidate_bytes = 0u64;

    for root in roots {
        tracing::info!("scanning library root");

        for entry in WalkDir::new(&root.path)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            let path = entry.path();

            if !entry.file_type().is_file() || !is_supported_image(path) {
                continue;
            }

            discovered_files += 1;

            match entry.metadata() {
                Ok(metadata) => {
                    let size_bytes = metadata.len();

                    if size_bytes < options.min_file_size_bytes {
                        skipped_by_file_size += 1;
                    } else {
                        candidate_bytes = candidate_bytes.saturating_add(size_bytes);
                        candidates.push(CandidatePhoto {
                            root: root.clone(),
                            path: path.to_path_buf(),
                            size_bytes,
                        });
                    }
                }
                Err(error) => {
                    tracing::warn!(?error, "failed to read image metadata for one candidate file");
                }
            }

            if discovered_files % WALK_PROGRESS_EVERY == 0 {
                let _ = event_sender.send(ScanEvent::Progress(ScanProgress {
                    phase: "Discovering images",
                    discovered_files,
                    candidate_files: candidates.len(),
                    processed_candidates: 0,
                    kept_images: 0,
                }));
            }
        }
    }

    let walk_ms = millis(walk_started.elapsed());
    let candidate_files = candidates.len();

    let _ = event_sender.send(ScanEvent::Progress(ScanProgress {
        phase: "Preparing duplicate checks",
        discovered_files,
        candidate_files,
        processed_candidates: 0,
        kept_images: 0,
    }));

    let sizes_to_hash = repeated_file_sizes(&candidates);

    let hash_candidate_files = candidates
        .iter()
        .filter(|candidate| sizes_to_hash.contains_key(&candidate.size_bytes))
        .count();

    let hash_candidate_bytes = candidates
        .iter()
        .filter(|candidate| sizes_to_hash.contains_key(&candidate.size_bytes))
        .map(|candidate| candidate.size_bytes)
        .sum();

    let skipped_by_dimensions = AtomicUsize::new(0);
    let hash_ms = AtomicU64::new(0);
    let processed_candidates = AtomicUsize::new(0);
    let kept_images = AtomicUsize::new(0);
    let failures = AtomicUsize::new(0);
    let record_started = Instant::now();

    candidates
        .into_par_iter()
        .enumerate()
        .for_each_with(event_sender.clone(), |sender, (index, candidate)| {
            match build_record(index as u64, candidate, &sizes_to_hash, options, &hash_ms) {
                Ok(Some(record)) => {
                    kept_images.fetch_add(1, Ordering::Relaxed);
                    let _ = sender.send(ScanEvent::PhotoFound(record));
                }
                Ok(None) => {
                    skipped_by_dimensions.fetch_add(1, Ordering::Relaxed);
                }
                Err(error) => {
                    failures.fetch_add(1, Ordering::Relaxed);
                    let _ = sender.send(ScanEvent::Failed(error));
                }
            }

            let processed = processed_candidates.fetch_add(1, Ordering::Relaxed) + 1;

            if processed % RECORD_PROGRESS_EVERY == 0 || processed == candidate_files {
                let _ = sender.send(ScanEvent::Progress(ScanProgress {
                    phase: "Reading metadata and hashes",
                    discovered_files,
                    candidate_files,
                    processed_candidates: processed,
                    kept_images: kept_images.load(Ordering::Relaxed),
                }));
            }
        });

    StreamingScanOutput {
        total_images: kept_images.load(Ordering::Relaxed),
        failures: failures.load(Ordering::Relaxed),
        stats: ScanStats {
            discovered_files,
            skipped_by_file_size,
            skipped_by_dimensions: skipped_by_dimensions.load(Ordering::Relaxed),
            candidate_bytes,
            hash_candidate_files,
            hash_candidate_bytes,
            walk_ms,
            record_build_ms: millis(record_started.elapsed()),
            hash_ms: hash_ms.load(Ordering::Relaxed) as u128,
        },
    }
}

fn repeated_file_sizes(candidates: &[CandidatePhoto]) -> HashMap<u64, usize> {
    let mut counts = HashMap::new();

    for candidate in candidates {
        *counts.entry(candidate.size_bytes).or_insert(0) += 1;
    }

    counts.retain(|_, count| *count > 1);
    counts
}

fn build_record(
    index: u64,
    candidate: CandidatePhoto,
    sizes_to_hash: &HashMap<u64, usize>,
    options: ScanOptions,
    hash_ms: &AtomicU64,
) -> Result<Option<PhotoRecord>, PhotoScoutError> {
    let file_name = candidate
        .path
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::to_owned)
        .ok_or_else(|| PhotoScoutError::MissingFileName(candidate.path.display().to_string()))?;

    let relative_path = candidate
        .path
        .strip_prefix(&candidate.root.path)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| candidate.path.clone());

    let (width, height) = match image::image_dimensions(&candidate.path) {
        Ok((width, height)) => {
            if (options.min_width > 0 && width < options.min_width)
                || (options.min_height > 0 && height < options.min_height)
            {
                return Ok(None);
            }

            (Some(width), Some(height))
        }
        Err(error) => {
            tracing::warn!(?error, "failed to read dimensions for one candidate image");
            (None, None)
        }
    };

    let content_hash = if sizes_to_hash.contains_key(&candidate.size_bytes) {
        let hash_started = Instant::now();
        let hash = hash_file(&candidate.path)?;
        hash_ms.fetch_add(millis(hash_started.elapsed()) as u64, Ordering::Relaxed);
        Some(hash)
    } else {
        None
    };

    let extension = lower_extension(&relative_path);

    Ok(Some(PhotoRecord {
        id: PhotoId(index),
        root_id: candidate.root.id,
        path: candidate.path,
        relative_path,
        file_name,
        extension,
        size_bytes: candidate.size_bytes,
        width,
        height,
        content_hash,
    }))
}

fn hash_file(path: &Path) -> Result<String, PhotoScoutError> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let bytes_read = file.read(&mut buffer)?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}
