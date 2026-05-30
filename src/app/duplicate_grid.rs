use super::helpers::{fit_inside, format_bytes, stack_paper_size, truncate_middle};
use super::{theme, PhotoScoutApp};
use crate::model::{PhotoId, PhotoRecord};
use eframe::egui;

impl PhotoScoutApp {
    pub(crate) fn filtered_duplicate_groups(&self) -> Vec<Vec<PhotoId>> {
        let needle = self.query.text.trim().to_ascii_lowercase();
        let mut groups = Vec::new();

        for group in self.duplicates.groups() {
            let mut group_matches = false;
            for id in group {
                let Some(photo) = self.photos_by_id.get(id) else {
                    continue;
                };

                if let Some(root_id) = self.query.root_filter {
                    if photo.root_id != root_id {
                        continue;
                    }
                }

                if needle.is_empty() {
                    group_matches = true;
                    break;
                }

                let path_text = photo.path.display().to_string().to_ascii_lowercase();
                let file_text = photo.file_name.to_ascii_lowercase();
                if file_text.contains(&needle) || path_text.contains(&needle) {
                    group_matches = true;
                    break;
                }
            }

            if group_matches {
                groups.push(group.clone());
            }
        }

        groups.sort_by_key(|group| {
            group
                .first()
                .and_then(|id| self.photos_by_id.get(id))
                .map(|photo| photo.file_name.to_ascii_lowercase())
                .unwrap_or_default()
        });
        groups
    }

    pub(crate) fn show_duplicate_stacks(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let groups = self.filtered_duplicate_groups();
        // Duplicate metrics are surfaced in the top command bar and left stats panel.
        // The center canvas starts directly with review cards for better accessibility.

        if groups.is_empty() {
            theme::card_frame().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.label(theme::muted("No duplicate stacks match this folder/search."));
                    ui.add_space(20.0);
                });
            });
            return;
        }

        let mut visible_photos = Vec::new();
        for group in &groups {
            for id in group {
                if let Some(photo) = self.photos_by_id.get(id) {
                    visible_photos.push(photo.clone());
                }
            }
        }

        egui::ScrollArea::vertical()
            .id_salt(("duplicate_review_cards", self.query.root_filter.map(|id| id.0)))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (group_index, group) in groups.iter().enumerate() {
                    let available_width = ui.available_width().max(1.0);
                    let card_width = (available_width - 10.0).clamp(260.0, 900.0).min(available_width);
                    let left_pad = ((available_width - card_width) / 2.0).max(0.0);

                    ui.horizontal_top(|ui| {
                        ui.add_space(left_pad);
                        ui.allocate_ui_with_layout(
                            egui::vec2(card_width, 1.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_width(card_width);
                                self.show_duplicate_review_card(ctx, ui, group_index + 1, group);
                            },
                        );
                    });
                    ui.add_space(14.0);
                }
            });

        if self.thumb_cache.visible_previews_ready(visible_photos.iter()) {
            self.thumb_cache.refine_visible_thumbnails(visible_photos.iter());
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    fn show_duplicate_review_card(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        group_number: usize,
        group: &[PhotoId],
    ) {
        let photos: Vec<PhotoRecord> = group
            .iter()
            .filter_map(|id| self.photos_by_id.get(id).cloned())
            .collect();

        if photos.is_empty() {
            return;
        }

        self.thumb_cache.prefetch_visible_neighbors(photos.iter());

        let first_id = photos[0].id;
        let selected_group = self
            .selected_duplicate_group
            .as_ref()
            .is_some_and(|ids| ids.first() == Some(&first_id));
        let keeper_id = self.keeper_for_group(group);
        let selected_id_for_details = self
            .selected_id
            .filter(|id| photos.iter().any(|photo| photo.id == *id))
            .or(keeper_id)
            .unwrap_or(first_id);
        let group_size: u64 = photos.iter().map(|photo| photo.size_bytes).sum();

        let frame = if selected_group {
            theme::selected_card_frame()
        } else {
            theme::warm_card_frame()
        };

        frame.show(ui, |ui| {
            let content_width = ui.available_width().max(1.0);
            ui.set_width(content_width);
            let compact = content_width < 660.0;

            ui.horizontal_wrapped(|ui| {
                ui.label(theme::heading(format!("Group {group_number}")));
                theme::pill_label(
                    ui,
                    format!("{} copies", photos.len()),
                    theme::SOFT_PURPLE,
                    theme::PURPLE,
                );
                theme::pill_label(
                    ui,
                    format_bytes(group_size),
                    theme::SOFT_PINK,
                    theme::HOT_PINK,
                );
            });

            ui.add_space(10.0);

            if compact {
                ui.vertical(|ui| {
                    let stack_width = ui.available_width().min(320.0);
                    let (rect, stack_response) = ui.allocate_exact_size(
                        egui::vec2(stack_width, 178.0),
                        egui::Sense::click(),
                    );
                    self.paint_duplicate_stack(ctx, ui, rect, &photos);
                    if stack_response.clicked() {
                        self.selected_duplicate_group = Some(group.to_vec());
                        self.selected_id = Some(selected_id_for_details);
                    }

                    ui.add_space(10.0);
                    for photo in &photos {
                        let keep = keeper_id == Some(photo.id);
                        self.show_duplicate_file_row(ui, group, photo, keep);
                        ui.add_space(6.0);
                    }

                    self.show_duplicate_cleanup_button(ui, keeper_id.is_some(), group);
                });
            } else {
                let inner_width = ui.available_width().max(1.0);
                let stack_width = 250.0_f32.min(inner_width * 0.38);
                let gap = 16.0;
                let row_width = (inner_width - stack_width - gap - 4.0).max(240.0);

                ui.horizontal_top(|ui| {
                    let (rect, stack_response) = ui.allocate_exact_size(
                        egui::vec2(stack_width, 190.0),
                        egui::Sense::click(),
                    );
                    self.paint_duplicate_stack(ctx, ui, rect, &photos);
                    if stack_response.clicked() {
                        self.selected_duplicate_group = Some(group.to_vec());
                        self.selected_id = Some(selected_id_for_details);
                    }

                    ui.add_space(gap);
                    ui.allocate_ui_with_layout(
                        egui::vec2(row_width.min(ui.available_width()), 1.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            for photo in &photos {
                                let keep = keeper_id == Some(photo.id);
                                self.show_duplicate_file_row(ui, group, photo, keep);
                                ui.add_space(6.0);
                            }

                            ui.add_space(2.0);
                            self.show_duplicate_cleanup_button(ui, keeper_id.is_some(), group);
                        },
                    );
                });
            }
        });
    }


    fn show_duplicate_cleanup_button(
        &mut self,
        ui: &mut egui::Ui,
        ready: bool,
        group: &[PhotoId],
    ) {
        let label = if ready { "Duplicate Cleanup" } else { "Select one to keep" };
        let width = ui.available_width().min(260.0).max(190.0);

        ui.horizontal_centered(|ui| {
            ui.add_enabled_ui(ready, |ui| {
                let response = ui.add_sized(
                    egui::vec2(width, 34.0),
                    egui::Button::new(
                        egui::RichText::new(label)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(theme::HOT_PINK)
                    .corner_radius(egui::CornerRadius::same(14)),
                );

                if response.clicked() {
                    self.open_cleanup_confirmation_for_group(group);
                }
            });
        });
    }

    fn show_duplicate_file_row(
        &mut self,
        ui: &mut egui::Ui,
        group: &[PhotoId],
        photo: &PhotoRecord,
        keep: bool,
    ) {
        // Keep and non-keep rows must reserve the same outer width.
        // The previous version set the inner Frame UI to the full available width;
        // after Frame margins/stroke were added, selected rows could become wider
        // than unselected rows and visually drift out of alignment.
        let outer_width = ui.available_width().max(160.0);
        let inner_width = (outer_width - 32.0).max(128.0);
        let name_chars = if outer_width < 260.0 { 22 } else if outer_width < 420.0 { 32 } else { 46 };
        let path_chars = if outer_width < 300.0 { 0 } else if outer_width < 420.0 { 38 } else { 66 };

        let frame = Self::duplicate_file_row_frame(keep);
        let mut clicked = false;
        let response = frame
            .show(ui, |ui| {
                ui.set_min_width(inner_width);
                ui.set_max_width(inner_width);
                ui.horizontal_top(|ui| {
                    let radio_response = ui.add(egui::RadioButton::new(keep, ""));
                    clicked |= radio_response.clicked();

                    ui.vertical(|ui| {
                        ui.set_min_width((inner_width - 28.0).max(96.0));
                        ui.set_max_width((inner_width - 28.0).max(96.0));

                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(truncate_middle(&photo.file_name, name_chars))
                                    .strong()
                                    .color(theme::TEXT),
                            );
                            if keep {
                                theme::pill_label(ui, "KEEP", theme::SOFT_PINK, theme::HOT_PINK);
                            }
                        });
                        ui.small(theme::muted(format!(
                            "{}  •  {} × {}  •  {}",
                            format_bytes(photo.size_bytes),
                            photo.width.map(|value| value.to_string()).unwrap_or_else(|| "?".to_string()),
                            photo.height.map(|value| value.to_string()).unwrap_or_else(|| "?".to_string()),
                            photo.extension.to_ascii_uppercase(),
                        )));
                        if path_chars > 0 {
                            ui.small(theme::muted(truncate_middle(&photo.relative_path.display().to_string(), path_chars)));
                        }
                    });
                });
            })
            .response
            .interact(egui::Sense::click());

        if clicked || response.clicked() {
            self.mark_duplicate_keeper(group, photo.id);
        }
    }

    fn duplicate_file_row_frame(keep: bool) -> egui::Frame {
    let fill = if keep { theme::SOFT_PINK } else { theme::CARD_BG };
    let stroke_color = if keep { theme::BORDER } else { theme::BORDER_SOFT };

    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke_color))
        .corner_radius(egui::CornerRadius::same(14))
        .inner_margin(egui::Margin::symmetric(14, 10))
    }

    fn paint_duplicate_stack(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        photos: &[PhotoRecord],
    ) {
        let painter = ui.painter();
        let base_x = if rect.width() < 230.0 { 4.0 } else { 18.0 };
        let offsets = [
            egui::vec2(base_x, 34.0),
            egui::vec2(base_x + 40.0, 12.0),
            egui::vec2(base_x + 78.0, 38.0),
            egui::vec2(base_x + 86.0, 20.0),
        ];

        let draw_count = photos.len().min(offsets.len());
        for index in 0..draw_count {
            let photo = &photos[index];
            let mut paper_size = stack_paper_size(photo);
            if paper_size.x > rect.width() - offsets[index].x - 8.0 {
                let scale = ((rect.width() - offsets[index].x - 8.0) / paper_size.x).clamp(0.5, 1.0);
                paper_size *= scale;
            }
            let top_left = rect.left_top() + offsets[index];
            let paper_rect = egui::Rect::from_min_size(top_left, paper_size);

            painter.rect_filled(paper_rect, 8.0, theme::CARD_BG);
            painter.rect_stroke(
                paper_rect,
                8.0,
                egui::Stroke::new(2.0, egui::Color32::WHITE),
                egui::StrokeKind::Outside,
            );
            painter.rect_stroke(
                paper_rect.expand(1.0),
                8.0,
                egui::Stroke::new(1.0, theme::BORDER_SOFT),
                egui::StrokeKind::Outside,
            );

            if let Some(render) = self.thumb_cache.texture_for_visible(ctx, photo) {
                let texture_id = render.texture.id();
                let fit_size = fit_inside(render.texture.size_vec2(), paper_rect.size() - egui::vec2(12.0, 12.0));
                let image_rect = egui::Rect::from_center_size(paper_rect.center(), fit_size);
                painter.image(
                    texture_id,
                    image_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            } else {
                painter.rect_filled(paper_rect.shrink(10.0), 4.0, theme::SOFT_PURPLE);
            }
        }

        if photos.len() > draw_count {
            let badge_rect = egui::Rect::from_min_size(
                rect.right_top() + egui::vec2(-58.0, 10.0),
                egui::vec2(50.0, 26.0),
            );
            painter.rect_filled(badge_rect, 13.0, theme::PURPLE);
            painter.text(
                badge_rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("+{}", photos.len() - draw_count),
                egui::FontId::proportional(13.0),
                egui::Color32::WHITE,
            );
        }
    }
}
