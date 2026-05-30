use eframe::egui;

pub const PURPLE: egui::Color32 = egui::Color32::from_rgb(136, 101, 255); // #8865FF
pub const HOT_PINK: egui::Color32 = egui::Color32::from_rgb(255, 111, 165); // #FF6FA5
pub const ROSE: egui::Color32 = egui::Color32::from_rgb(251, 158, 196); // #FB9EC4
pub const PEACH: egui::Color32 = egui::Color32::from_rgb(255, 218, 192); // #FFDAC0
pub const BLUSH: egui::Color32 = egui::Color32::from_rgb(255, 199, 186); // #FFC7BA

pub const APP_BG: egui::Color32 = egui::Color32::from_rgb(255, 247, 250);
pub const PANEL_BG: egui::Color32 = egui::Color32::from_rgb(255, 250, 253);
pub const SIDE_BG: egui::Color32 = egui::Color32::from_rgb(252, 246, 255);
pub const CARD_BG: egui::Color32 = egui::Color32::from_rgb(255, 253, 254);
pub const CARD_WARM: egui::Color32 = egui::Color32::from_rgb(255, 249, 246);
pub const SOFT_PINK: egui::Color32 = egui::Color32::from_rgb(255, 239, 246);
pub const SOFT_PURPLE: egui::Color32 = egui::Color32::from_rgb(243, 236, 255);
pub const SOFT_PEACH: egui::Color32 = egui::Color32::from_rgb(255, 244, 236);
pub const TEXT: egui::Color32 = egui::Color32::from_rgb(37, 35, 55);
pub const MUTED_TEXT: egui::Color32 = egui::Color32::from_rgb(112, 101, 126);
pub const BORDER: egui::Color32 = egui::Color32::from_rgb(248, 203, 220);
pub const BORDER_SOFT: egui::Color32 = egui::Color32::from_rgb(249, 224, 233);

pub fn install(ctx: &egui::Context) {
    // PhotoScout is used on large desktop displays; nudge the whole UI up a bit
    // while still respecting users who already run a larger OS/DPI scale.
    ctx.set_pixels_per_point(ctx.pixels_per_point().max(1.12));

    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 9.0);
    style.spacing.button_padding = egui::vec2(16.0, 8.0);
    style.spacing.menu_margin = egui::Margin::same(10);
    style.spacing.window_margin = egui::Margin::same(10);

    style.text_styles.insert(egui::TextStyle::Heading, egui::FontId::proportional(22.0));
    style.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(14.5));
    style.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
    style.text_styles.insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
    style.visuals.window_fill = APP_BG;
    style.visuals.panel_fill = APP_BG;
    style.visuals.extreme_bg_color = CARD_BG;
    style.visuals.faint_bg_color = SOFT_PINK;
    style.visuals.widgets.noninteractive.bg_fill = CARD_BG;
    style.visuals.widgets.noninteractive.fg_stroke.color = TEXT;
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(255, 247, 250);
    style.visuals.widgets.inactive.bg_stroke.color = BORDER_SOFT;
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(255, 236, 244);
    style.visuals.widgets.hovered.bg_stroke.color = ROSE;
    style.visuals.widgets.active.bg_fill = SOFT_PURPLE;
    style.visuals.widgets.active.bg_stroke.color = PURPLE;
    style.visuals.selection.bg_fill = SOFT_PURPLE;
    style.visuals.selection.stroke.color = PURPLE;
    style.visuals.hyperlink_color = PURPLE;
    ctx.set_global_style(style);
}

pub fn section_title(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text.into())
        .size(13.0)
        .strong()
        .color(PURPLE)
}

pub fn heading(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text.into())
        .size(21.0)
        .strong()
        .color(TEXT)
}

pub fn muted(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text.into()).color(MUTED_TEXT)
}

pub fn brand_title() -> egui::RichText {
    egui::RichText::new("PhotoScout")
        .size(30.0)
        .strong()
        .color(HOT_PINK)
}

pub fn brand_subtitle() -> egui::RichText {
    egui::RichText::new("by Rusty Island")
        .size(13.0)
        .color(TEXT)
}

pub fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(CARD_BG)
        .stroke(egui::Stroke::new(1.0, BORDER_SOFT))
        .corner_radius(egui::CornerRadius::same(14))
        .inner_margin(egui::Margin::same(14))
}

pub fn warm_card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(CARD_WARM)
        .stroke(egui::Stroke::new(1.0, PEACH))
        .corner_radius(egui::CornerRadius::same(14))
        .inner_margin(egui::Margin::same(14))
}

pub fn selected_card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(SOFT_PURPLE)
        .stroke(egui::Stroke::new(2.0, PURPLE))
        .corner_radius(egui::CornerRadius::same(14))
        .inner_margin(egui::Margin::same(12))
}


pub fn stat_card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(SOFT_PEACH)
        .stroke(egui::Stroke::new(1.0, BLUSH))
        .corner_radius(egui::CornerRadius::same(14))
        .inner_margin(egui::Margin::same(14))
}

pub fn section_card<R>(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    card_frame()
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(section_title(title));
            ui.add_space(4.0);
            add_contents(ui)
        })
        .inner
}

pub fn plain_click_text(text: impl Into<String>, selected: bool) -> egui::RichText {
    egui::RichText::new(text.into())
        .color(if selected { HOT_PINK } else { TEXT })
        .strong()
}

pub fn pink_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(egui::RichText::new(text).strong().color(egui::Color32::WHITE))
        .fill(HOT_PINK)
        .corner_radius(egui::CornerRadius::same(14))
}

pub fn subtle_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(egui::RichText::new(text).strong().color(PURPLE))
        .fill(SOFT_PURPLE)
        .corner_radius(egui::CornerRadius::same(14))
}

pub fn pill_label(ui: &mut egui::Ui, text: impl Into<String>, fill: egui::Color32, color: egui::Color32) {
    let text = text.into();
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, BORDER_SOFT))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(13.0).strong().color(color));
        });
}
