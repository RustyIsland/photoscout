use super::types::{ThumbnailResult, ThumbnailStage};
use crate::diagnostics::{self, ThumbnailBenchReport};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(super) struct ThumbnailBenchAccumulator {
    generation_id: u64,
    window_started: Instant,
    preview_completed: usize,
    final_completed: usize,
    preview_failures: usize,
    final_failures: usize,
    preview_decode_ms: u128,
    preview_resize_ms: u128,
    final_decode_ms: u128,
    final_resize_ms: u128,
    preview_uploaded: usize,
    final_uploaded: usize,
}

impl ThumbnailBenchAccumulator {
    pub(super) fn for_generation(generation_id: u64) -> Self {
        Self {
            generation_id,
            window_started: Instant::now(),
            preview_completed: 0,
            final_completed: 0,
            preview_failures: 0,
            final_failures: 0,
            preview_decode_ms: 0,
            preview_resize_ms: 0,
            final_decode_ms: 0,
            final_resize_ms: 0,
            preview_uploaded: 0,
            final_uploaded: 0,
        }
    }

    pub(super) fn record_result(&mut self, result: &ThumbnailResult) {
        let success = result.result.is_ok();
        match result.stage {
            ThumbnailStage::Preview => {
                if success {
                    self.preview_completed += 1;
                } else {
                    self.preview_failures += 1;
                }
                self.preview_decode_ms += result.decode_ms;
                self.preview_resize_ms += result.resize_ms;
            }
            ThumbnailStage::Final => {
                if success {
                    self.final_completed += 1;
                } else {
                    self.final_failures += 1;
                }
                self.final_decode_ms += result.decode_ms;
                self.final_resize_ms += result.resize_ms;
            }
        }
    }

    pub(super) fn record_upload(&mut self, stage: ThumbnailStage) {
        match stage {
            ThumbnailStage::Preview => self.preview_uploaded += 1,
            ThumbnailStage::Final => self.final_uploaded += 1,
        }
    }

    pub(super) fn maybe_report(&mut self, deferred_results: usize, pending_previews: usize, pending_finals: usize) {
        let elapsed = self.window_started.elapsed();
        if elapsed < Duration::from_secs(2) {
            return;
        }

        let seconds = elapsed.as_secs_f64();
        let preview_total = self.preview_completed + self.preview_failures;
        let final_total = self.final_completed + self.final_failures;

        if preview_total == 0 && final_total == 0 && self.preview_uploaded == 0 && self.final_uploaded == 0 {
            self.window_started = Instant::now();
            return;
        }

        diagnostics::log_thumbnail_report(ThumbnailBenchReport {
            generation_id: self.generation_id,
            preview_completed: self.preview_completed,
            final_completed: self.final_completed,
            preview_failures: self.preview_failures,
            final_failures: self.final_failures,
            preview_avg_decode_ms: avg_ms(self.preview_decode_ms, preview_total),
            preview_avg_resize_ms: avg_ms(self.preview_resize_ms, preview_total),
            final_avg_decode_ms: avg_ms(self.final_decode_ms, final_total),
            final_avg_resize_ms: avg_ms(self.final_resize_ms, final_total),
            preview_per_sec: self.preview_completed as f64 / seconds,
            final_per_sec: self.final_completed as f64 / seconds,
            preview_uploaded: self.preview_uploaded,
            final_uploaded: self.final_uploaded,
            deferred_results,
            pending_previews,
            pending_finals,
        });

        *self = Self::for_generation(self.generation_id);
    }
}

fn avg_ms(total_ms: u128, count: usize) -> f64 {
    if count == 0 {
        0.0
    } else {
        total_ms as f64 / count as f64
    }
}
