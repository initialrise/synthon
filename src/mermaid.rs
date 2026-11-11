use eframe::egui::ColorImage;

/// Render mermaid source to an image no wider than `max_width_px` (never upscaled).
/// `scale` = pixels_per_point × zoom, so output is crisp.
pub fn render(_src: &str, _dark: bool, _max_width_px: u32, _scale: f32) -> Result<ColorImage, String> {
    todo!("D: merman → resvg → ColorImage")
}

/// Load system fonts once (call from a background thread at startup).
pub fn warm_up() {}
