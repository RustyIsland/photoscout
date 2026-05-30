mod cleanup;
mod duplicate_grid;
mod grid;
mod helpers;
mod panels;
mod theme;

use crate::diagnostics;
use crate::duplicates::DuplicateIndex;
use crate::model::{LibraryRoot, LibraryRootId, PhotoId, PhotoRecord, ScanOptions, SearchQuery};
use crate::scan_coordinator::{start_scan, ScanMessage};
use crate::thumbnails::ThumbnailCache;
use eframe::egui;
use self::cleanup::CleanupDialog;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub(crate) struct ScanSummary {
    pub kept: usize,
    pub discovered: usize,
    pub skipped_file_size: usize,
    pub skipped_dimensions: usize,
    pub duplicate_photos: usize,
    pub duplicate_groups: usize,
    pub failures: usize,
}

pub struct PhotoScoutApp {
    pub(crate) roots: Vec<LibraryRoot>,
    pub(crate) next_root_id: u64,
    pub(crate) photos: Vec<PhotoRecord>,
    pub(crate) photos_by_id: HashMap<PhotoId, PhotoRecord>,
    pub(crate) duplicates: DuplicateIndex,
    pub(crate) query: SearchQuery,
    pub(crate) selected_id: Option<PhotoId>,
    pub(crate) selected_duplicate_group: Option<Vec<PhotoId>>,
    pub(crate) scan_receiver: Option<Receiver<ScanMessage>>,
    pub(crate) scan_status: String,
    pub(crate) last_scan_summary: Option<ScanSummary>,
    pub(crate) is_scanning: bool,
    pub(crate) thumb_cache: ThumbnailCache,
    pub(crate) last_error: Option<String>,
    pub(crate) min_file_size_kb: u64,
    pub(crate) min_width: u32,
    pub(crate) min_height: u32,
    pub(crate) current_scan_roots: usize,
    pub(crate) duplicate_keep_by_hash: HashMap<String, PhotoId>,
    pub(crate) cleanup_dialog: CleanupDialog,
    pub(crate) cleanup_reviewed: bool,
    pub(crate) ui_freeze_until: Option<Instant>,
}

impl PhotoScoutApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        Self {
            roots: Vec::new(),
            next_root_id: 1,
            photos: Vec::new(),
            photos_by_id: HashMap::new(),
            duplicates: DuplicateIndex::default(),
            query: SearchQuery::default(),
            selected_id: None,
            selected_duplicate_group: None,
            scan_receiver: None,
            scan_status: "Add one or more folders, then scan.".to_string(),
            last_scan_summary: None,
            is_scanning: false,
            thumb_cache: ThumbnailCache::default(),
            last_error: None,
            min_file_size_kb: 20,
            min_width: 128,
            min_height: 128,
            current_scan_roots: 0,
            duplicate_keep_by_hash: HashMap::new(),
            cleanup_dialog: CleanupDialog::Hidden,
            cleanup_reviewed: false,
            ui_freeze_until: None,
        }
    }

    pub(crate) fn add_folders(&mut self) -> bool {
        let Some(paths) = rfd::FileDialog::new()
            .set_title("Select and scan photo folders")
            .pick_folders()
        else {
            return false;
        };

        let before = self.roots.len();
        for path in paths {
            self.add_root_path(path);
        }

        self.roots.len() != before
    }

    fn add_root_path(&mut self, path: PathBuf) {
        let path = fs::canonicalize(&path).unwrap_or(path);
        if self.roots.iter().any(|root| root.path == path) {
            return;
        }

        let root = LibraryRoot::new(LibraryRootId(self.next_root_id), path);
        self.next_root_id += 1;
        self.roots.push(root);
    }

    pub(crate) fn select_and_scan_folders(&mut self) {
        let changed = self.add_folders();
        if changed && !self.roots.is_empty() {
            self.start_scan();
        }
    }

    fn scan_options(&self) -> ScanOptions {
        ScanOptions {
            min_file_size_bytes: self.min_file_size_kb.saturating_mul(1024),
            min_width: self.min_width,
            min_height: self.min_height,
        }
    }

    pub(crate) fn start_scan(&mut self) {
        if self.is_scanning {
            return;
        }

        self.photos.clear();
        self.photos_by_id.clear();
        self.duplicates = DuplicateIndex::default();
        self.selected_id = None;
        self.selected_duplicate_group = None;
        self.duplicate_keep_by_hash.clear();
        self.cleanup_dialog = CleanupDialog::Hidden;
        self.cleanup_reviewed = false;
        self.last_error = None;
        self.last_scan_summary = None;
        self.thumb_cache.clear();

        self.scan_receiver = Some(start_scan(self.roots.clone(), self.scan_options()));
        self.is_scanning = true;
        self.scan_status = "Scanning...".to_string();
    }

    fn drain_scan_messages_limited(&mut self, ctx: &egui::Context, max_messages_per_frame: usize) {
        let mut finished = false;
        let mut drained_messages = 0usize;

        if let Some(receiver) = &self.scan_receiver {
            while drained_messages < max_messages_per_frame {
                let Ok(message) = receiver.try_recv() else {
                    break;
                };

                drained_messages += 1;
                match message {
                    ScanMessage::Started { roots } => {
                        self.current_scan_roots = roots;
                        self.scan_status = format!("Scanning {roots} folder group(s)…");
                    }
                    ScanMessage::Progress {
                        phase,
                        discovered_files,
                        candidate_files,
                        processed_candidates,
                        kept_images,
                    } => {
                        self.scan_status = if processed_candidates > 0 {
                            format!(
                                "{phase}… {processed_candidates}/{candidate_files} processed · {kept_images} kept"
                            )
                        } else {
                            format!(
                                "{phase}… {discovered_files} discovered · {candidate_files} candidates"
                            )
                        };
                    }
                    ScanMessage::PhotoFound(photo) => {
                        self.photos_by_id.insert(photo.id, photo.clone());
                        self.photos.push(photo);
                        if self.photos.len() % 50 == 0 {
                            self.scan_status = format!("Scanning… {} kept", self.photos.len());
                        }
                    }
                    ScanMessage::Failed { error } => {
                        self.last_error = Some(error);
                    }
                    ScanMessage::Finished {
                        total_images,
                        failures,
                        stats,
                    } => {
                        self.scan_status = "Building duplicate index...".to_string();
                        self.duplicates = DuplicateIndex::rebuild(&self.photos);
                        diagnostics::log_scan_report(
                            total_images,
                            failures,
                            self.current_scan_roots,
                            stats,
                        );
                        self.last_scan_summary = Some(ScanSummary {
                            kept: total_images,
                            discovered: stats.discovered_files,
                            skipped_file_size: stats.skipped_by_file_size,
                            skipped_dimensions: stats.skipped_by_dimensions,
                            duplicate_photos: self.duplicates.duplicate_photo_count(),
                            duplicate_groups: self.duplicates.duplicate_group_count(),
                            failures,
                        });
                        self.scan_status = "Scan complete".to_string();
                        self.is_scanning = false;
                        finished = true;
                    }
                }
            }
        }

        if finished {
            self.scan_receiver = None;
        }

        if self.is_scanning || drained_messages == max_messages_per_frame {
            ctx.request_repaint();
        }
    }

    fn update_interaction_freeze(&mut self, ctx: &egui::Context) -> bool {
        let pointer_moving = ctx.input(|input| {
            input.pointer.any_down()
                && input.pointer.delta().length_sq() > 0.5
        });

        if pointer_moving {
            self.ui_freeze_until = Some(Instant::now() + Duration::from_millis(120));
        }

        let freeze_heavy_ui = self
            .ui_freeze_until
            .is_some_and(|until| Instant::now() < until);

        if !freeze_heavy_ui {
            self.ui_freeze_until = None;
        }

        freeze_heavy_ui
    }

    pub(crate) fn refresh_scan_summary_from_current_state(&mut self) {
        if let Some(summary) = &mut self.last_scan_summary {
            summary.kept = self.photos.len();
            summary.duplicate_photos = self.duplicates.duplicate_photo_count();
            summary.duplicate_groups = self.duplicates.duplicate_group_count();
        }
    }

    pub(crate) fn selected_photo(&self) -> Option<&PhotoRecord> {
        let id = self.selected_id?;
        self.photos_by_id.get(&id)
    }

    pub(crate) fn root_photo_counts(&self) -> HashMap<LibraryRootId, usize> {
        let mut counts = HashMap::new();
        for photo in &self.photos {
            *counts.entry(photo.root_id).or_insert(0) += 1;
        }
        counts
    }

    fn show_duplicate_mode_button(&mut self, ui: &mut egui::Ui) {
        let duplicate_mode = self.query.duplicates_only;
        let duplicate_groups = self.duplicates.duplicate_group_count();

        let label = if duplicate_mode {
            if duplicate_groups > 0 {
                format!("Duplicate Review · {duplicate_groups} groups")
            } else {
                "Duplicate Review · ON".to_string()
            }
        } else if duplicate_groups > 0 {
            format!("Duplicate Review · {duplicate_groups}")
        } else {
            "Duplicate Review".to_string()
        };

        let button = if duplicate_mode {
            egui::Button::new(
                egui::RichText::new(label)
                    .size(14.0)
                    .strong()
                    .color(egui::Color32::WHITE),
            )
            .fill(theme::HOT_PINK)
            .corner_radius(egui::CornerRadius::same(18))
        } else {
            egui::Button::new(
                egui::RichText::new(label)
                    .size(14.0)
                    .strong()
                    .color(theme::PURPLE),
            )
            .fill(theme::SOFT_PURPLE)
            .corner_radius(egui::CornerRadius::same(18))
        };

        let response = ui
            .add_sized(egui::vec2(210.0, 38.0), button)
            .on_hover_text(if duplicate_mode {
                "Click to return to all photos."
            } else {
                "Click to review exact duplicate groups."
            });

        if response.clicked() {
            self.query.duplicates_only = !self.query.duplicates_only;
            self.selected_id = None;
            self.selected_duplicate_group = None;
        }
    }
}

impl eframe::App for PhotoScoutApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let freeze_heavy_ui = self.update_interaction_freeze(&ctx);
        let (max_preview_uploads, max_final_uploads, max_scan_messages) = if freeze_heavy_ui {
            // While the user is dragging/resizing panels or the window, keep the UI responsive by
            // reducing texture uploads and scan-message bursts for a few frames.
            (6, 1, 30)
        } else {
            (48, 6, 250)
        };

        self.thumb_cache.begin_frame();
        self.thumb_cache
            .drain_completed(&ctx, max_preview_uploads, max_final_uploads);
        self.drain_scan_messages_limited(&ctx, max_scan_messages);

        egui::Panel::top("top_bar").show_inside(ui, |ui| {
            egui::Frame::new()
                .fill(theme::PANEL_BG)
                .inner_margin(egui::Margin::symmetric(18, 12))
                .show(ui, |ui| {
                    ui.set_min_height(76.0);
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(theme::brand_title());
                            ui.label(theme::brand_subtitle());
                        });

                        ui.add_space(18.0);

                        let select_enabled = !self.is_scanning;
                        if ui
                            .add_enabled(
                                select_enabled,
                                egui::Button::new(
                                    egui::RichText::new("+ Select & Scan Folder")
                                        .size(15.5)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                )
                                .fill(theme::PURPLE)
                                .corner_radius(egui::CornerRadius::same(18))
                                .min_size(egui::vec2(220.0, 38.0)),
                            )
                            .clicked()
                        {
                            self.select_and_scan_folders();
                        }

                        ui.add_space(8.0);

                        let remaining = ui.available_width();
                        let search_width = (remaining - 230.0).clamp(260.0, 620.0);
                        ui.add_sized(
                            egui::vec2(search_width, 38.0),
                            egui::TextEdit::singleline(&mut self.query.text)
                                .hint_text("Search photos, folders, or file names...")
                                .font(egui::FontId::proportional(16.0))
                                .vertical_align(egui::Align::Center)
                                .margin(egui::Margin::symmetric(12, 8)),
                        );

                        ui.add_space(8.0);
                        self.show_duplicate_mode_button(ui);
                    });
                });
        });

        egui::Panel::left("left_panel")
            .resizable(true)
            .min_size(210.0)
            .show_inside(ui, |ui| {
                egui::Frame::new()
                    .fill(theme::SIDE_BG)
                    .inner_margin(egui::Margin::symmetric(10, 10))
                    .show(ui, |ui| self.show_left_panel(ui));
            });

        egui::Panel::right("right_panel")
            .resizable(true)
            .min_size(255.0)
            .show_inside(ui, |ui| {
                egui::Frame::new()
                    .fill(theme::PANEL_BG)
                    .inner_margin(egui::Margin::symmetric(10, 10))
                    .show(ui, |ui| self.show_right_panel(ui));
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::APP_BG)
                    .inner_margin(egui::Margin::symmetric(14, 12)),
            )
            .show_inside(ui, |ui| self.show_photo_grid(&ctx, ui));

        self.show_cleanup_dialog(&ctx);
    }
}