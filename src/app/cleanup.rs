use super::{theme, PhotoScoutApp};
use super::helpers::{format_bytes, truncate_middle};
use crate::duplicates::DuplicateIndex;
use crate::model::PhotoId;
use eframe::egui;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub(crate) struct CleanupPlan {
    pub(crate) keep_ids: Vec<PhotoId>,
    pub(crate) trash_ids: Vec<PhotoId>,
    pub(crate) total_bytes: u64,
    pub(crate) affected_roots: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CleanupResult {
    pub(crate) moved_count: usize,
    pub(crate) failed_count: usize,
    pub(crate) moved_bytes: u64,
    pub(crate) errors: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) enum CleanupDialog {
    #[default]
    Hidden,
    Confirm(CleanupPlan),
    Complete(CleanupResult),
    Failed(String),
}

impl PhotoScoutApp {
    pub(crate) fn mark_duplicate_keeper(&mut self, group: &[PhotoId], keeper_id: PhotoId) {
        let Some(group_hash) = self.hash_for_group(group) else {
            return;
        };

        if !group.iter().any(|id| *id == keeper_id) {
            return;
        }

        self.duplicate_keep_by_hash.insert(group_hash, keeper_id);
        self.selected_duplicate_group = Some(group.to_vec());
        self.selected_id = Some(keeper_id);
    }

    pub(crate) fn keeper_for_group(&self, group: &[PhotoId]) -> Option<PhotoId> {
        let group_hash = self.hash_for_group(group)?;
        let keeper = self.duplicate_keep_by_hash.get(&group_hash).copied()?;
        group.iter().any(|id| *id == keeper).then_some(keeper)
    }

    pub(crate) fn cleanup_plan_for_group(&self, group: &[PhotoId]) -> CleanupPlan {
        let mut keep_ids = Vec::new();
        let mut trash_ids = Vec::new();
        let mut affected_roots = HashSet::new();
        let mut total_bytes = 0_u64;

        let Some(keeper) = self.keeper_for_group(group) else {
            return CleanupPlan::default();
        };

        for id in group {
            if *id == keeper {
                continue;
            }

            if let Some(photo) = self.photos_by_id.get(id) {
                trash_ids.push(*id);
                affected_roots.insert(photo.root_id);
                total_bytes = total_bytes.saturating_add(photo.size_bytes);
            }
        }

        if !trash_ids.is_empty() {
            keep_ids.push(keeper);
        }

        CleanupPlan {
            keep_ids,
            trash_ids,
            total_bytes,
            affected_roots: affected_roots.len(),
        }
    }

    pub(crate) fn open_cleanup_confirmation_for_group(&mut self, group: &[PhotoId]) {
        let plan = self.cleanup_plan_for_group(group);
        if plan.trash_ids.is_empty() {
            self.cleanup_dialog = CleanupDialog::Failed(
                "Select one keeper inside this duplicate group before moving files to Trash.".to_string(),
            );
            return;
        }

        self.cleanup_reviewed = false;
        self.cleanup_dialog = CleanupDialog::Confirm(plan);
    }

    pub(crate) fn perform_cleanup_to_trash(&mut self, plan: CleanupPlan) {
        let mut moved_ids = HashSet::new();
        let mut moved_bytes = 0_u64;
        let mut errors = Vec::new();

        for id in &plan.trash_ids {
            let Some(photo) = self.photos_by_id.get(id).cloned() else {
                continue;
            };

            let trash_path = path_for_trash(&photo.path);
            match trash::delete(&trash_path) {
                Ok(()) => {
                    moved_ids.insert(*id);
                    moved_bytes = moved_bytes.saturating_add(photo.size_bytes);
                }
                Err(error) => {
                    errors.push(format!("{}: {error}", display_path_for_user(&photo.path)));
                }
            }
        }

        if !moved_ids.is_empty() {
            self.photos.retain(|photo| !moved_ids.contains(&photo.id));
            self.photos_by_id.retain(|id, _| !moved_ids.contains(id));
            self.duplicates = DuplicateIndex::rebuild(&self.photos);
            self.duplicate_keep_by_hash.retain(|_, keeper_id| self.photos_by_id.contains_key(keeper_id));

            if self.selected_id.is_some_and(|id| moved_ids.contains(&id)) {
                self.selected_id = None;
                self.selected_duplicate_group = None;
            }

            self.thumb_cache.forget_photos(&moved_ids);
            self.refresh_scan_summary_from_current_state();
        }

        let result = CleanupResult {
            moved_count: moved_ids.len(),
            failed_count: errors.len(),
            moved_bytes,
            errors,
        };

        if result.moved_count == 0 && result.failed_count > 0 {
            self.cleanup_dialog = CleanupDialog::Failed(
                format!(
                    "Could not move files to Trash. No files were permanently deleted. {}",
                    result.errors.first().cloned().unwrap_or_default()
                ),
            );
        } else {
            self.cleanup_dialog = CleanupDialog::Complete(result);
        }
    }

    pub(crate) fn show_cleanup_dialog(&mut self, ctx: &egui::Context) {
        let dialog = self.cleanup_dialog.clone();
        if matches!(dialog, CleanupDialog::Hidden) {
            return;
        }

        let screen_rect = ctx.content_rect();

        // Modal behavior: consume the full screen area first so the user must
        // choose Cancel/Confirm/Done inside the card before returning to the app.
        egui::Area::new(egui::Id::new("cleanup_modal_overlay_input"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen_rect.min)
            .show(ctx, |ui| {
                let (rect, _response) = ui.allocate_exact_size(screen_rect.size(), egui::Sense::click());
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(255, 250, 253, 218),
                );
            });

        let card_width = (screen_rect.width() - 48.0).clamp(360.0, 620.0);
        let card_height = match &dialog {
            CleanupDialog::Confirm(_) => 460.0,
            CleanupDialog::Complete(_) | CleanupDialog::Failed(_) | CleanupDialog::Hidden => 300.0,
        };
        let card_size = egui::vec2(card_width, card_height);
        let card_pos = screen_rect.center() - card_size / 2.0;

        egui::Area::new(egui::Id::new("cleanup_modal_card"))
            .order(egui::Order::Tooltip)
            .fixed_pos(card_pos)
            .show(ctx, |ui| {
                ui.set_width(card_size.x);
                egui::Frame::new()
                    .fill(theme::CARD_BG)
                    .stroke(egui::Stroke::new(1.5, theme::BORDER))
                    .corner_radius(egui::CornerRadius::same(18))
                    .inner_margin(egui::Margin::same(18))
                    .show(ui, |ui| {
                        ui.set_width(card_size.x - 36.0);
                        match dialog {
                            CleanupDialog::Hidden => {}
                            CleanupDialog::Confirm(plan) => self.show_confirm_cleanup_card(ui, plan),
                            CleanupDialog::Complete(result) => self.show_cleanup_complete_card(ui, result),
                            CleanupDialog::Failed(message) => self.show_cleanup_failed_card(ui, &message),
                        }
                    });
            });
    }

    fn show_confirm_cleanup_card(&mut self, ui: &mut egui::Ui, plan: CleanupPlan) {
        ui.vertical_centered(|ui| {
            ui.label(theme::heading("Duplicate Cleanup"));
            ui.add_space(8.0);
            ui.label(theme::muted(format!(
                "This will send {} duplicate file(s) to Trash and keep {} selected file(s).",
                plan.trash_ids.len(),
                plan.keep_ids.len()
            )));
            ui.add_space(6.0);
            ui.label(format!("Affected folder group(s): {}", plan.affected_roots));
            ui.label(format!("Estimated reclaimable size: {}", format_bytes(plan.total_bytes)));
            ui.add_space(8.0);
            ui.label(theme::muted(
                "Trash is recoverable; PhotoScout never permanently deletes files.",
            ));
            ui.add_space(10.0);
            ui.label(theme::section_title("FILES THAT WILL BE SENT TO TRASH"));
        });

        egui::Frame::new()
            .fill(theme::SOFT_PINK)
            .stroke(egui::Stroke::new(1.0, theme::BORDER))
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical()
                    .max_height(108.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for id in &plan.trash_ids {
                            if let Some(photo) = self.photos_by_id.get(id) {
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        egui::RichText::new(truncate_middle(&photo.file_name, 54))
                                            .strong()
                                            .color(theme::TEXT),
                                    );
                                    ui.small(theme::muted(truncate_middle(&display_path_for_user(&photo.path), 82)))
                                        .on_hover_text(display_path_for_user(&photo.path));
                                });
                                ui.add_space(6.0);
                            }
                        }
                    });
            });

        ui.add_space(10.0);
        ui.vertical_centered(|ui| {
            ui.checkbox(&mut self.cleanup_reviewed, "I understand this will send the listed duplicate file(s) to Trash.");
            ui.add_space(12.0);

            let confirm_text = format!("Send {} Other Duplicate(s) to Trash", plan.trash_ids.len());
            let cancel_width = 92.0;
            let confirm_width = 286.0;
            let gap = 12.0;
            let total_width = cancel_width + gap + confirm_width;
            let left_pad = ((ui.available_width() - total_width) / 2.0).max(0.0);

            ui.horizontal(|ui| {
                ui.add_space(left_pad);
                if ui
                    .add_sized(egui::vec2(cancel_width, 32.0), theme::subtle_button("Cancel"))
                    .clicked()
                {
                    self.cleanup_dialog = CleanupDialog::Hidden;
                    self.cleanup_reviewed = false;
                }

                ui.add_space(gap);
                if ui
                    .add_enabled(
                        self.cleanup_reviewed,
                        theme::pink_button(&confirm_text).min_size(egui::vec2(confirm_width, 32.0)),
                    )
                    .clicked()
                {
                    self.cleanup_reviewed = false;
                    self.perform_cleanup_to_trash(plan);
                }
            });
        });
    }

    fn show_cleanup_complete_card(&mut self, ui: &mut egui::Ui, result: CleanupResult) {
        ui.vertical_centered(|ui| {
            ui.label(theme::heading("Duplicate Cleanup Complete"));
            ui.add_space(8.0);
            ui.label(format!("Sent to Trash: {} file(s)", result.moved_count));
            ui.label(format!("Estimated recovered size: {}", format_bytes(result.moved_bytes)));

            if result.failed_count > 0 {
                ui.add_space(8.0);
                ui.colored_label(
                    theme::HOT_PINK,
                    format!("{} file(s) could not be moved. No permanent delete fallback was used.", result.failed_count),
                );
                for error in result.errors.iter().take(3) {
                    ui.small(theme::muted(truncate_middle(error, 82))).on_hover_text(error);
                }
            }

            ui.add_space(14.0);
            let button_width = 140.0;
            let left_pad = ((ui.available_width() - button_width) / 2.0).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(left_pad);
                if ui
                    .add_sized(egui::vec2(button_width, 32.0), theme::pink_button("Done"))
                    .clicked()
                {
                    self.cleanup_dialog = CleanupDialog::Hidden;
                }
            });
        });
    }

    fn show_cleanup_failed_card(&mut self, ui: &mut egui::Ui, message: &str) {
        ui.vertical_centered(|ui| {
            ui.label(theme::heading("Could not move files to Trash"));
            ui.add_space(8.0);
            ui.colored_label(theme::HOT_PINK, "No files were permanently deleted.");
            ui.add_space(8.0);
            ui.label(theme::muted(truncate_middle(message, 110))).on_hover_text(message);
            ui.add_space(14.0);
            let button_width = 140.0;
            let left_pad = ((ui.available_width() - button_width) / 2.0).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(left_pad);
                if ui
                    .add_sized(egui::vec2(button_width, 32.0), theme::pink_button("OK"))
                    .clicked()
                {
                    self.cleanup_dialog = CleanupDialog::Hidden;
                }
            });
        });
    }

    fn hash_for_group(&self, group: &[PhotoId]) -> Option<String> {
        group
            .iter()
            .filter_map(|id| self.photos_by_id.get(id))
            .find_map(|photo| photo.content_hash.clone())
    }

}

#[cfg(windows)]
fn path_for_trash(path: &Path) -> PathBuf {
    let value = path.display().to_string();
    if let Some(stripped) = value.strip_prefix("\\\\?\\UNC\\") {
        PathBuf::from(format!("\\\\{stripped}"))
    } else if let Some(stripped) = value.strip_prefix("\\\\?\\") {
        PathBuf::from(stripped)
    } else {
        path.to_path_buf()
    }
}

#[cfg(not(windows))]
fn path_for_trash(path: &Path) -> PathBuf {
    path.to_path_buf()
}

fn display_path_for_user(path: &Path) -> String {
    path_for_trash(path).display().to_string()
}
