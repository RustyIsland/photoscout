use crate::model::PhotoRecord;
use eframe::egui;

pub(crate) fn stack_paper_size(photo: &PhotoRecord) -> egui::Vec2 {
    match (photo.width, photo.height) {
        (Some(width), Some(height)) if height > width => egui::vec2(102.0, 146.0),
        (Some(width), Some(height)) if width > height => egui::vec2(150.0, 110.0),
        _ => egui::vec2(124.0, 124.0),
    }
}

pub(crate) fn fit_inside(original: egui::Vec2, bounds: egui::Vec2) -> egui::Vec2 {
    if original.x <= 0.0 || original.y <= 0.0 {
        return bounds;
    }

    let scale = (bounds.x / original.x).min(bounds.y / original.y);
    egui::vec2(original.x * scale, original.y * scale)
}

pub(crate) fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let value = bytes as f64;
    if value >= GB {
        format!("{:.2} GB", value / GB)
    } else if value >= MB {
        format!("{:.2} MB", value / MB)
    } else if value >= KB {
        format!("{:.1} KB", value / KB)
    } else {
        format!("{bytes} B")
    }
}

pub(crate) fn truncate_middle(value: &str, max_chars: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= max_chars {
        return value.to_string();
    }

    let keep = max_chars.saturating_sub(3) / 2;
    let start: String = chars.iter().take(keep).collect();
    let end: String = chars
        .iter()
        .rev()
        .take(keep)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{start}...{end}")
}
