mod cli;
mod doc;
mod highlight;
mod keys;
mod mermaid;
mod parse;
mod table;
mod view;

use clap::Parser;
use std::time::Instant;

fn main() {
    let start = Instant::now();
    let args = cli::Args::parse();
    let input = args.validate().and_then(|_| cli::read_input(&args));
    let (title, text) = input.unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let blocks = parse::parse(&text);
    drop(text);

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(format!("{title} — synthon"))
            .with_app_id("synthon")
            .with_inner_size([900.0, 1000.0]),
        ..Default::default()
    };
    let theme = match (args.dark, args.light) {
        (true, _) => Some(eframe::egui::Theme::Dark),
        (_, true) => Some(eframe::egui::Theme::Light),
        _ => None, // follow OS
    };
    let res = eframe::run_native(
        "synthon",
        options,
        Box::new(move |cc| {
            if let Some(t) = theme {
                cc.egui_ctx.set_theme(t);
            }
            let mut v = view::Viewer::new(&cc.egui_ctx, blocks, args.full_width, args.zoom as f32 / 100.0);
            if std::env::var_os("SYNTHON_BENCH").is_some() {
                v.bench = Some(start);
            }
            Ok(Box::new(v))
        }),
    );
    if let Err(e) = res {
        eprintln!("synthon: {e}");
        std::process::exit(1);
    }
}
