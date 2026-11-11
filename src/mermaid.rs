#![allow(dead_code)]

use eframe::egui::ColorImage;
use merman::svg::{SvgPipeline, SvgRenderOptions};
use merman::{
    MermaidEngine, OperationControl, RenderOutput, RenderRequest, Renderer,
    SourcePresentationConfig, SvgRequest,
};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{self, fontdb};
use std::sync::{Arc, OnceLock};

const MAX_SIDE: f32 = 8192.0;

static FONTS: OnceLock<Arc<fontdb::Database>> = OnceLock::new();

fn fonts() -> Arc<fontdb::Database> {
    FONTS
        .get_or_init(|| {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone()
}

/// Load the system font database now, so the first `render` doesn't pay for it.
pub fn warm_up() {
    fonts();
}

fn to_svg(src: &str, dark: bool) -> Result<String, String> {
    let theme = if dark { "dark" } else { "default" };
    let cfg = SourcePresentationConfig::from_value(serde_json::json!({ "theme": theme }));
    let renderer =
        Renderer::new().with_presentation_engine(MermaidEngine::new().with_site_config(cfg));
    let request = SvgRequest {
        options: SvgRenderOptions {
            diagram_id: Some("d".into()),
            ..Default::default()
        },
        pipeline: Some(SvgPipeline::resvg_safe()),
        ..Default::default()
    };
    match renderer.render(RenderRequest::svg(src, OperationControl::new(), request)) {
        Ok(RenderOutput::Svg(Some(s))) => Ok(s.into_parts().0),
        Ok(_) => Err("no mermaid diagram detected".into()),
        Err(e) => Err(e.to_string()),
    }
}

/// Render mermaid source to an image no wider than `max_width_px` (never upscaled).
/// `scale` = pixels_per_point × zoom, so output is crisp.
pub fn render(src: &str, dark: bool, max_width_px: u32, scale: f32) -> Result<ColorImage, String> {
    let svg = to_svg(src, dark)?;
    let opts = usvg::Options {
        fontdb: fonts(),
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(&svg, &opts).map_err(|e| e.to_string())?;
    let (nw, nh) = (tree.size().width(), tree.size().height());
    let s = scale
        .min(max_width_px as f32 / nw)
        .min(MAX_SIDE / nw)
        .min(MAX_SIDE / nh);
    let (w, h) = ((nw * s).round().max(1.0), (nh * s).round().max(1.0));
    let mut pixmap = Pixmap::new(w as u32, h as u32).ok_or("mermaid: bad image size")?;
    resvg::render(&tree, Transform::from_scale(s, s), &mut pixmap.as_mut());
    Ok(ColorImage::from_rgba_premultiplied(
        [w as usize, h as usize],
        pixmap.data(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOW: &str = "flowchart LR\n  A[Start] --> B{Ok?}\n  B -->|yes| C[Done]\n  B -->|no| A\n";

    #[test]
    fn flowchart_fits_max_width() {
        let img = render(FLOW, false, 120, 2.0).unwrap();
        assert!(img.size[0] <= 120 && img.size[1] >= 1, "{:?}", img.size);
        assert!(img.pixels.iter().any(|p| p.a() > 0));
    }

    #[test]
    fn never_upscaled() {
        let tree = usvg::Tree::from_str(&to_svg(FLOW, true).unwrap(), &Default::default()).unwrap();
        let natural = tree.size().width().round() as usize;
        let wide = render(FLOW, true, 100_000, 1.0).unwrap().size;
        assert_eq!(wide[0], natural);
        let doubled = render(FLOW, true, 100_000, 2.0).unwrap().size;
        assert!(doubled[0].abs_diff(natural * 2) <= 1);
    }

    #[test]
    fn garbage_errs() {
        assert!(render("garbage", false, 800, 1.0).is_err());
    }
}

