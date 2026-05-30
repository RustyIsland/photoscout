use crate::error::PhotoScoutError;
use crate::library_roots::prune_nested_roots;
use crate::model::{LibraryRoot, PhotoRecord, ScanOptions, ScanStats};
use crate::scanner::{scan_roots_streaming, ScanEvent};
use std::sync::mpsc::{self, Receiver};
use std::thread;

#[derive(Debug)]
pub enum ScanMessage {
    Started { roots: usize },
    Progress {
        phase: &'static str,
        discovered_files: usize,
        candidate_files: usize,
        processed_candidates: usize,
        kept_images: usize,
    },
    PhotoFound(PhotoRecord),
    Failed { error: String },
    Finished {
        total_images: usize,
        failures: usize,
        stats: ScanStats,
    },
}

pub fn start_scan(roots: Vec<LibraryRoot>, options: ScanOptions) -> Receiver<ScanMessage> {
    let (ui_sender, ui_receiver) = mpsc::channel();

    thread::spawn(move || {
        let roots = prune_nested_roots(&roots);
        let _ = ui_sender.send(ScanMessage::Started { roots: roots.len() });

        if roots.is_empty() {
            let _ = ui_sender.send(ScanMessage::Finished {
                total_images: 0,
                failures: 0,
                stats: ScanStats::default(),
            });
            return;
        }

        let (scan_sender, scan_receiver) = mpsc::channel();
        let ui_sender_for_events = ui_sender.clone();
        let forwarder = thread::spawn(move || {
            for event in scan_receiver {
                match event {
                    ScanEvent::Progress(progress) => {
                        let _ = ui_sender_for_events.send(ScanMessage::Progress {
                            phase: progress.phase,
                            discovered_files: progress.discovered_files,
                            candidate_files: progress.candidate_files,
                            processed_candidates: progress.processed_candidates,
                            kept_images: progress.kept_images,
                        });
                    }
                    ScanEvent::PhotoFound(photo) => {
                        if ui_sender_for_events.send(ScanMessage::PhotoFound(photo)).is_err() {
                            break;
                        }
                    }
                    ScanEvent::Failed(error) => {
                        let _ = ui_sender_for_events.send(ScanMessage::Failed {
                            error: format_error(error),
                        });
                    }
                }
            }
        });

        let scan_output = scan_roots_streaming(&roots, options, scan_sender);
        let _ = forwarder.join();

        let _ = ui_sender.send(ScanMessage::Finished {
            total_images: scan_output.total_images,
            failures: scan_output.failures,
            stats: scan_output.stats,
        });
    });

    ui_receiver
}

fn format_error(error: PhotoScoutError) -> String {
    match error {
        PhotoScoutError::Io(error) => format!("filesystem error: {error}"),
        PhotoScoutError::Image(error) => format!("image decoding error: {error}"),
        PhotoScoutError::MissingFileName(_) => "path has no file name".to_string(),
    }
}
