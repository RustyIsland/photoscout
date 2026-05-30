use super::helpers::{fit_inside, format_bytes, truncate_middle};
use super::{theme, PhotoScoutApp};
use crate::model::PhotoRecord;
use crate::search::filter_indices;
use eframe::egui;

const TILE_WIDTH: f32 = 232.0;
const TILE_HEIGHT: f32 = 282.0;
const IMAGE_BOX_SIZE: f32 = 184.0;
const TILE_X_SPACING: f32 = 16.0;
const TILE_Y_SPACING: f32 = 16.0;

impl PhotoScoutApp {
    pub(crate) fn show_photo_grid(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let indices = filter_indices(&self.photos, &self.duplicates, &self.query);
        // Keep the central canvas focused on images. Counts and duplicate metrics live in
        // the top command bar / side stats so this area does not waste vertical space.

        if self.photos.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label("Add folders and scan to begin.");
            });
            return;
        }

        if indices.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label("No photos match the current search/filter.");
            });
            return;
        }

        if self.query.duplicates_only {
            self.show_duplicate_stacks(ctx, ui);
            return;
        }

        let available_width = ui.available_width().max(TILE_WIDTH);
        let tile_stride = TILE_WIDTH + TILE_X_SPACING;
        let columns = (available_width / tile_stride).floor().max(1.0) as usize;
        let row_count = indices.len().div_ceil(columns);

        egui::ScrollArea::vertical()
            .id_salt(("photo_grid", columns))
            .auto_shrink([false, false])
            .show_rows(ui, TILE_HEIGHT + TILE_Y_SPACING, row_count, |ui, row_range| {
                let visible_start = row_range.start.saturating_mul(columns);
                let visible_end = (row_range.end.saturating_mul(columns)).min(indices.len());

                let visible_photos: Vec<PhotoRecord> = indices[visible_start..visible_end]
                    .iter()
                    .map(|&photo_index| self.photos[photo_index].clone())
                    .collect();

                for row_index in row_range.clone() {
                    let start = row_index * columns;
                    let end = (start + columns).min(indices.len());

                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = TILE_X_SPACING;
                        let actual_columns = end.saturating_sub(start).max(1);
                        let row_width = actual_columns as f32 * (TILE_WIDTH - 14.0)
                            + actual_columns.saturating_sub(1) as f32 * TILE_X_SPACING;
                        let left_pad = ((ui.available_width() - row_width) / 2.0).max(0.0);
                        ui.add_space(left_pad);
                        for &photo_index in &indices[start..end] {
                            let photo = self.photos[photo_index].clone();
                            ui.allocate_ui_with_layout(
                                egui::vec2(TILE_WIDTH - 14.0, TILE_HEIGHT - 14.0),
                                egui::Layout::top_down(egui::Align::Center),
                                |ui| self.show_photo_tile(ctx, ui, &photo),
                            );
                        }
                    });
                }

                if self.thumb_cache.visible_previews_ready(visible_photos.iter()) {
                    self.thumb_cache.refine_visible_thumbnails(visible_photos.iter());
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_millis(16));
                }

                let prefetch_start = visible_start.saturating_sub(columns * 2);
                let prefetch_end = (visible_end + columns * 2).min(indices.len());
                let mut prefetch_photos = Vec::new();
                for &photo_index in &indices[prefetch_start..visible_start] {
                    prefetch_photos.push(self.photos[photo_index].clone());
                }
                for &photo_index in &indices[visible_end..prefetch_end] {
                    prefetch_photos.push(self.photos[photo_index].clone());
                }
                self.thumb_cache.prefetch_visible_neighbors(prefetch_photos.iter());
            });
    }

    pub(crate) fn show_photo_tile(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, photo: &PhotoRecord) {
        let image_box = egui::vec2(IMAGE_BOX_SIZE, IMAGE_BOX_SIZE);
        let is_selected = self.selected_id == Some(photo.id);
        let is_duplicate = self.duplicates.is_duplicate(photo.id);

        let tile_frame = if is_selected {
            theme::selected_card_frame()
        } else {
            theme::card_frame()
        };

        tile_frame.show(ui, |ui| {
                ui.set_width(184.0);
                ui.set_height(246.0);

                let thumb_response = ui.allocate_ui_with_layout(
                    image_box,
                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                    |ui| {
                        if let Some(render) = self.thumb_cache.texture_for_visible(ctx, photo) {
                            let texture_id = render.texture.id();
                            let texture_size = render.texture.size_vec2();
                            let is_preview = render.is_preview;
                            let fit_size = fit_inside(texture_size, image_box);
                            let image = egui::Image::new((texture_id, fit_size))
                                .sense(egui::Sense::click());
                            let response = ui.add(image);
                            let _ = is_preview;
                            response
                        } else {
                            ui.add_sized(image_box, egui::Button::new("Loading…"))
                        }
                    },
                );

                if thumb_response.inner.clicked() || thumb_response.response.clicked() {
                    self.selected_id = Some(photo.id);
                    self.selected_duplicate_group = None;
                }

                ui.add_space(4.0);
                let name_response = ui.add(
                    egui::Label::new(theme::plain_click_text(truncate_middle(&photo.file_name, 24), is_selected))
                        .sense(egui::Sense::click()),
                );
                if name_response.clicked() {
                    self.selected_id = Some(photo.id);
                    self.selected_duplicate_group = None;
                }

                ui.small(format_bytes(photo.size_bytes));
                if is_duplicate {
                    ui.colored_label(theme::HOT_PINK, "duplicate");
                }
            });
    }
}
