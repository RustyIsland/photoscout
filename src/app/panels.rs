use super::helpers::{fit_inside, format_bytes, truncate_middle};
use super::{theme, PhotoScoutApp};
use crate::model::SortMode;
use eframe::egui;

impl PhotoScoutApp {
    pub(crate) fn show_left_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                theme::section_card(ui, "SCAN OVERVIEW", |ui| {
                    ui.label(if self.is_scanning { "Scanning…" } else { self.scan_status.as_str() });
                    if let Some(summary) = self.last_scan_summary {
                        summary_row(ui, "Kept", &format!("{} / {}", summary.kept, summary.discovered));
                        summary_row(ui, "Skipped", &format!("{} size · {} dimensions", summary.skipped_file_size, summary.skipped_dimensions));
                        summary_row(ui, "Duplicates", &format!("{} in {} groups", summary.duplicate_photos, summary.duplicate_groups));
                        summary_row(ui, "Failures", &summary.failures.to_string());
                    } else if !self.is_scanning {
                        ui.small(theme::muted("Select and scan a folder to begin."));
                    }
                    if let Some(error) = &self.last_error {
                        ui.colored_label(theme::HOT_PINK, format!("Last warning: {error}"));
                    }
                });

                ui.add_space(10.0);
                theme::section_card(ui, "LIBRARY", |ui| {
                    ui.small(theme::muted("Click a folder to show only photos from that folder."));
                    ui.add_space(5.0);

                    let all_selected = self.query.root_filter.is_none();
                    if nav_count_row(ui, "📚", "All photos", self.photos.len(), all_selected, None).clicked() {
                        self.query.root_filter = None;
                        self.selected_id = None;
                        self.selected_duplicate_group = None;
                    }

                    if self.roots.is_empty() {
                        ui.label(theme::muted("No folders added yet."));
                    }

                    let mut remove_index = None;
                    let root_counts = self.root_photo_counts();
                    for (index, root) in self.roots.iter().enumerate() {
                        let selected = self.query.root_filter == Some(root.id);
                        let count = root_counts.get(&root.id).copied().unwrap_or(0);
                        ui.horizontal(|ui| {
                            let row_width = (ui.available_width() - 26.0).max(80.0);
                            ui.allocate_ui_with_layout(
                                egui::vec2(row_width, 28.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    let response = nav_count_row(ui, "📁", &root.label, count, selected, None)
                                        .on_hover_text(root.path.display().to_string());
                                    if response.clicked() {
                                        self.query.root_filter = Some(root.id);
                                        self.selected_id = None;
                                        self.selected_duplicate_group = None;
                                    }
                                },
                            );
                            if ui.small_button("×").on_hover_text("Remove folder from library").clicked() {
                                remove_index = Some(index);
                            }
                        });
                    }

                    if let Some(index) = remove_index {
                        let removed = self.roots.remove(index);
                        if self.query.root_filter == Some(removed.id) {
                            self.query.root_filter = None;
                        }
                    }
                });

                ui.add_space(10.0);
                theme::section_card(ui, "THRESHOLDS", |ui| {
                    threshold_row_u64(ui, "Min file size", &mut self.min_file_size_kb, "KB");
                    threshold_row_u32(ui, "Min width", &mut self.min_width, "px");
                    threshold_row_u32(ui, "Min height", &mut self.min_height, "px");
                    ui.small(theme::muted("Higher values reduce noise and small assets."));
                });

                ui.add_space(10.0);
                theme::stat_card_frame().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(theme::section_title("STATS"));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Images");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.strong(self.photos.len().to_string());
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Duplicate photos");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.strong(self.duplicates.duplicate_photo_count().to_string());
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Duplicate groups");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.strong(self.duplicates.duplicate_group_count().to_string());
                        });
                    });
                });

                ui.add_space(10.0);
                theme::section_card(ui, "SORT BY", |ui| {
                    ui.radio_value(&mut self.query.sort_mode, SortMode::NameAsc, "Name A-Z");
                    ui.radio_value(&mut self.query.sort_mode, SortMode::SizeLargest, "Largest first");
                    ui.radio_value(&mut self.query.sort_mode, SortMode::RootThenName, "Folder then name");
                });
            });
    }

    pub(crate) fn show_right_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(theme::section_title("SELECTED PHOTO"));
                ui.add_space(6.0);

                let Some(photo) = self.selected_photo().cloned() else {
                    theme::card_frame().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(theme::muted("Select an image or duplicate stack to see details."));
                    });
                    return;
                };

                theme::card_frame().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let preview_box = egui::vec2(ui.available_width(), 240.0);
                    ui.allocate_ui_with_layout(
                        preview_box,
                        egui::Layout::centered_and_justified(egui::Direction::TopDown),
                        |ui| {
                            if let Some(render) = self.thumb_cache.texture_for_visible(ui.ctx(), &photo) {
                                let fit = fit_inside(render.texture.size_vec2(), preview_box - egui::vec2(16.0, 16.0));
                                ui.add(egui::Image::new((render.texture.id(), fit)));
                            } else {
                                ui.add_sized(preview_box, egui::Button::new("Loading preview…"));
                            }
                        },
                    );
                });

                ui.add_space(10.0);
                theme::section_card(ui, "FILE INFO", |ui| {
                    info_row(ui, "Name", &photo.file_name);
                    info_row(ui, "Path", &photo.path.display().to_string());
                    info_row(ui, "Size", &format_bytes(photo.size_bytes));
                    match (photo.width, photo.height) {
                        (Some(width), Some(height)) => info_row(ui, "Dimensions", &format!("{width} × {height}")),
                        _ => info_row(ui, "Dimensions", "unknown"),
                    };
                    info_row(ui, "Type", &photo.extension.to_ascii_uppercase());
                    if let Some(hash) = &photo.content_hash {
                        info_row(ui, "BLAKE3", &truncate_middle(hash, 36));
                    } else {
                        info_row(ui, "BLAKE3", "not hashed; unique file size");
                    }
                });

                ui.add_space(10.0);
                theme::section_card(ui, "ACTIONS", |ui| {
                    if ui
                        .add_sized(egui::vec2(ui.available_width(), 32.0), theme::pink_button("Open file"))
                        .clicked()
                    {
                        if let Err(error) = open::that(&photo.path) {
                            self.last_error = Some(error.to_string());
                        }
                    }

                    ui.horizontal(|ui| {
                        if ui.add(theme::subtle_button("Open folder")).clicked() {
                            if let Some(parent) = photo.path.parent() {
                                if let Err(error) = open::that(parent) {
                                    self.last_error = Some(error.to_string());
                                }
                            }
                        }

                        if ui.add(theme::subtle_button("Copy path")).clicked() {
                            ui.ctx().copy_text(photo.path.display().to_string());
                        }
                    });
                });

                ui.add_space(10.0);
                theme::section_card(ui, "DUPLICATE STATUS", |ui| {
                    if let Some(group) = self.duplicates.group_for(&photo) {
                        theme::pill_label(ui, format!("{} copies", group.len()), theme::SOFT_PINK, theme::HOT_PINK);
                        ui.small(theme::muted("Exact duplicate group detected."));
                    } else {
                        ui.label("No exact duplicate group detected.");
                    }
                });

            });
    }
}

fn info_row(ui: &mut egui::Ui, label: &str, value: &str) {
    egui::Grid::new(format!("info_row_{label}"))
        .num_columns(2)
        .spacing(egui::vec2(10.0, 4.0))
        .show(ui, |ui| {
            ui.add_sized(
                egui::vec2(82.0, 18.0),
                egui::Label::new(egui::RichText::new(label).color(theme::MUTED_TEXT).size(13.0)),
            );
            ui.label(egui::RichText::new(truncate_middle(value, 42)).color(theme::TEXT).size(13.0))
                .on_hover_text(value);
            ui.end_row();
        });
}

fn summary_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("•").color(theme::HOT_PINK));
        ui.add_sized(
            egui::vec2(74.0, 18.0),
            egui::Label::new(egui::RichText::new(label).color(theme::MUTED_TEXT).size(12.5)),
        );
        ui.label(egui::RichText::new(value).color(theme::TEXT).size(12.5));
    });
}

fn threshold_row_u64(ui: &mut egui::Ui, label: &str, value: &mut u64, unit: &str) {
    ui.horizontal(|ui| {
        ui.add_sized(egui::vec2(92.0, 20.0), egui::Label::new(label));
        ui.add_sized(
            egui::vec2(54.0, 20.0),
            egui::DragValue::new(value).range(0..=1_048_576),
        );
        ui.label(unit);
    });
}

fn threshold_row_u32(ui: &mut egui::Ui, label: &str, value: &mut u32, unit: &str) {
    ui.horizontal(|ui| {
        ui.add_sized(egui::vec2(92.0, 20.0), egui::Label::new(label));
        ui.add_sized(
            egui::vec2(54.0, 20.0),
            egui::DragValue::new(value).range(0..=100_000),
        );
        ui.label(unit);
    });
}

fn nav_count_row(
    ui: &mut egui::Ui,
    icon: &str,
    label: &str,
    count: usize,
    selected: bool,
    trailing: Option<&str>,
) -> egui::Response {
    let available = ui.available_width();
    let height = 28.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(available, height), egui::Sense::click());

    let fill = if selected { theme::SOFT_PURPLE } else { theme::CARD_BG };
    let stroke = if selected {
        egui::Stroke::new(1.0, theme::BORDER)
    } else {
        egui::Stroke::new(1.0, theme::BORDER_SOFT)
    };
    ui.painter().rect_filled(rect, 9.0, fill);
    ui.painter().rect_stroke(rect, 9.0, stroke, egui::StrokeKind::Outside);

    let text_color = if selected { theme::PURPLE } else { theme::TEXT };
    let left = rect.left() + 10.0;
    let center_y = rect.center().y;
    ui.painter().text(
        egui::pos2(left, center_y),
        egui::Align2::LEFT_CENTER,
        icon,
        egui::FontId::proportional(13.0),
        text_color,
    );
    ui.painter().text(
        egui::pos2(left + 24.0, center_y),
        egui::Align2::LEFT_CENTER,
        truncate_middle(label, 20),
        egui::FontId::proportional(13.0),
        text_color,
    );

    let count_x = rect.right() - if trailing.is_some() { 36.0 } else { 12.0 };
    ui.painter().text(
        egui::pos2(count_x, center_y),
        egui::Align2::RIGHT_CENTER,
        count.to_string(),
        egui::FontId::proportional(13.0),
        theme::MUTED_TEXT,
    );

    if let Some(trailing) = trailing {
        let small_rect = egui::Rect::from_center_size(
            egui::pos2(rect.right() - 13.0, center_y),
            egui::vec2(18.0, 18.0),
        );
        ui.painter().rect_filled(small_rect, 6.0, theme::BORDER_SOFT);
        ui.painter().text(
            small_rect.center(),
            egui::Align2::CENTER_CENTER,
            trailing,
            egui::FontId::proportional(12.0),
            theme::MUTED_TEXT,
        );
    }

    response
}
