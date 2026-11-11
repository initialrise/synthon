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
    let bench = std::env::var_os("SYNTHON_BENCH").is_some();
    // Parse, load fonts and warm syntect while the window and GL context come up (~70 ms).
    let prep = std::thread::spawn(move || {
        let blocks = parse::parse(&text);
        let mut firsts: Vec<(String, String)> = Vec::new();
        doc::walk(&blocks, &mut |b| {
            if let doc::Block::Code { lang: Some(l), text } = b
                && !firsts.iter().any(|(fl, _)| fl == l)
            {
                firsts.push((l.clone(), text.clone()));
            }
        });
        std::thread::spawn(move || {
            for (lang, text) in firsts {
                highlight::highlight(&text, Some(&lang), true, eframe::egui::FontId::monospace(14.0));
            }
        });
        blocks
    });
    let fonts = std::thread::spawn(view::load_fonts);

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
            if bench {
                eprintln!("bench: window+gl ready {:.1} ms", start.elapsed().as_secs_f64() * 1e3);
            }
            let blocks = prep.join().expect("parser thread");
            let fonts = fonts.join().expect("font thread");
            let mut v = view::Viewer::new(&cc.egui_ctx, blocks, fonts, args.full_width, args.zoom as f32 / 100.0);
            if bench {
                eprintln!("bench: viewer+fonts {:.1} ms", start.elapsed().as_secs_f64() * 1e3);
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
