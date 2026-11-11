//! F · View: lay out every block once per (width, ppp, theme), paint only what's visible.
//! All block heights are always known, so no frame ever shows unlaid content.

use crate::doc::{Align, Block, Span};
use crate::keys::{self, Action};
use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Galley, Pos2, Rect, Sense, Stroke, TextFormat,
    TextureHandle, TextureOptions, Vec2, scroll_area::ScrollSource, text::LayoutJob,
    text_selection::LabelSelectionState,
};
use std::ops::Range;
use std::sync::{Arc, atomic::AtomicU64, atomic::Ordering, mpsc};

const BODY: f32 = 16.0;
const GAP: f32 = 12.0;
const PAD: f32 = 24.0;
const READING_WIDTH: f32 = 46.0 * BODY; // ~80ch
const CELL_PAD: f32 = 8.0;

fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

// ---------- layout model ----------

enum Item {
    Text { pos: Vec2, galley: Arc<Galley>, links: Vec<(Range<usize>, String)> },
    Rect { rect: Rect, fill: Color32, stroke: Stroke },
    /// Code block: background `rect`, galley may be wider → horizontal scroll inside.
    Code { rect: Rect, galley: Arc<Galley> },
    Mermaid { rect: Rect, id: usize },
}

struct Laid {
    y: f32,
    h: f32,
    items: Vec<Item>,
}

#[derive(Clone, Copy, PartialEq)]
struct Key {
    col_w: u32,
    ppp: u32,
    dark: bool,
}

struct Colors {
    dark: bool,
    text: Color32,
    weak: Color32,
    link: Color32,
    code_bg: Color32,
    border: Color32,
    head_bg: Color32,
    err: Color32,
}

impl Colors {
    fn new(dark: bool) -> Self {
        let c = Color32::from_rgb;
        if dark {
            Colors {
                dark,
                text: c(0xe6, 0xed, 0xf3),
                weak: c(0x8b, 0x94, 0x9e),
                link: c(0x58, 0xa6, 0xff),
                code_bg: c(0x16, 0x1b, 0x22),
                border: c(0x30, 0x36, 0x3d),
                head_bg: c(0x1c, 0x21, 0x28),
                err: c(0xf8, 0x51, 0x49),
            }
        } else {
            Colors {
                dark,
                text: c(0x1f, 0x23, 0x28),
                weak: c(0x59, 0x63, 0x6e),
                link: c(0x09, 0x69, 0xda),
                code_bg: c(0xf6, 0xf8, 0xfa),
                border: c(0xd1, 0xd9, 0xe0),
                head_bg: c(0xf6, 0xf8, 0xfa),
                err: c(0xcf, 0x22, 0x2e),
            }
        }
    }
    fn bg(dark: bool) -> Color32 {
        if dark { Color32::from_rgb(0x0d, 0x11, 0x17) } else { Color32::WHITE }
    }
}

// ---------- mermaid ----------

#[derive(Default)]
struct Slot {
    /// Last good render (kept while a re-render for a new theme/zoom is in flight).
    tex: Option<(TextureHandle, Vec2)>,
    err: Option<String>,
}

struct Job {
    gen_: u64,
    id: usize,
    src: String,
    dark: bool,
    scale: f32,
}

type Done = (u64, usize, Result<egui::ColorImage, String>, f32);

fn spawn_worker(ctx: egui::Context, gen_: Arc<AtomicU64>) -> (mpsc::Sender<Job>, mpsc::Receiver<Done>) {
    let (jtx, jrx) = mpsc::channel::<Job>();
    let (dtx, drx) = mpsc::channel::<Done>();
    std::thread::spawn(move || {
        crate::mermaid::warm_up();
        for j in jrx {
            if j.gen_ != gen_.load(Ordering::Relaxed) {
                continue; // stale: theme/zoom changed since it was queued
            }
            // Render at natural size × scale; the view downsizes on the GPU if the column is narrower.
            let r = std::panic::catch_unwind(|| crate::mermaid::render(&j.src, j.dark, 8192, j.scale))
                .unwrap_or_else(|_| Err("renderer panicked".into()));
            if dtx.send((j.gen_, j.id, r, j.scale)).is_err() {
                break;
            }
            ctx.request_repaint();
        }
    });
    (jtx, drx)
}

fn collect_mermaid(blocks: &[Block], out: &mut Vec<String>) {
    for b in blocks {
        match b {
            Block::Mermaid(s) => out.push(s.clone()),
            Block::Quote(bs) | Block::Footnote { blocks: bs, .. } => collect_mermaid(bs, out),
            Block::List { items, .. } => items.iter().for_each(|i| collect_mermaid(&i.blocks, out)),
            _ => {}
        }
    }
}

// ---------- viewer ----------

pub struct Viewer {
    blocks: Vec<Block>,
    /// First mermaid id inside each top-level block.
    mermaid_start: Vec<usize>,
    /// Top-level block index for each mermaid id.
    mermaid_owner: Vec<usize>,
    mermaid_src: Vec<String>,
    slots: Vec<Slot>,
    gen_: Arc<AtomicU64>,
    jobs: mpsc::Sender<Job>,
    done: mpsc::Receiver<Done>,
    mermaid_key: Option<(bool, u32)>,

    full_width: bool,
    zoom: f32,
    key: Option<Key>,
    laid: Vec<Laid>,
    total_h: f32,
    col_w: f32,

    offset: f32,
    pending_offset: Option<f32>,
    view_h: f32,
    /// (top-level block, fraction into it) at the top of the viewport.
    anchor: (usize, f32),
    /// SYNTHON_BENCH: process start; prints first-frame and diagrams-ready times, then quits.
    pub bench: Option<std::time::Instant>,
    frames: u32,
}

impl Viewer {
    pub fn new(ctx: &egui::Context, blocks: Vec<Block>, full_width: bool, zoom: f32) -> Self {
        install_fonts(ctx);
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        ctx.set_zoom_factor(zoom);

        let mut mermaid_src = Vec::new();
        let mut mermaid_start = Vec::with_capacity(blocks.len());
        let mut mermaid_owner = Vec::new();
        for (i, b) in blocks.iter().enumerate() {
            mermaid_start.push(mermaid_src.len());
            collect_mermaid(std::slice::from_ref(b), &mut mermaid_src);
            mermaid_owner.resize(mermaid_src.len(), i);
        }
        let gen_ = Arc::new(AtomicU64::new(0));
        let (jobs, done) = spawn_worker(ctx.clone(), gen_.clone());
        Viewer {
            slots: mermaid_src.iter().map(|_| Slot::default()).collect(),
            blocks,
            mermaid_start,
            mermaid_owner,
            mermaid_src,
            gen_,
            jobs,
            done,
            mermaid_key: None,
            full_width,
            zoom,
            key: None,
            laid: Vec::new(),
            total_h: 0.0,
            col_w: 0.0,
            offset: 0.0,
            pending_offset: None,
            view_h: 600.0,
            anchor: (0, 0.0),
            bench: None,
            frames: 0,
        }
    }

    fn apply(&mut self, ctx: &egui::Context, a: Action) {
        let line = BODY * 1.5;
        let page = (self.view_h - 2.0 * line).max(line);
        let mut scroll = |d: f32| {
            let max = (self.total_h - self.view_h).max(0.0);
            let base = self.pending_offset.unwrap_or(self.offset);
            self.pending_offset = Some((base + d).clamp(0.0, max));
        };
        match a {
            Action::ScrollLines(n) => scroll(n as f32 * line),
            Action::HalfPage(n) => scroll(n as f32 * self.view_h / 2.0),
            Action::Page(n) => scroll(n as f32 * page),
            Action::Top => scroll(-1e9),
            Action::Bottom => scroll(1e9),
            Action::ToggleTheme => ctx.set_theme(if ctx.theme() == egui::Theme::Dark {
                egui::Theme::Light
            } else {
                egui::Theme::Dark
            }),
            Action::ToggleFullWidth => self.full_width = !self.full_width,
            Action::ZoomIn => self.set_zoom(ctx, self.zoom + 0.1),
            Action::ZoomOut => self.set_zoom(ctx, self.zoom - 0.1),
            Action::ZoomReset => self.set_zoom(ctx, 1.0),
            Action::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }

    fn set_zoom(&mut self, ctx: &egui::Context, z: f32) {
        self.zoom = (z * 10.0).round().clamp(5.0, 30.0) / 10.0;
        ctx.set_zoom_factor(self.zoom);
    }

    /// Queue every diagram for (re)rendering when theme or pixel density changes.
    fn sync_mermaid(&mut self, dark: bool, ppp: f32) {
        let k = (dark, ppp.to_bits());
        if self.mermaid_key == Some(k) {
            return;
        }
        self.mermaid_key = Some(k);
        let g = self.gen_.fetch_add(1, Ordering::Relaxed) + 1;
        for (id, src) in self.mermaid_src.iter().enumerate() {
            let _ = self.jobs.send(Job { gen_: g, id, src: src.clone(), dark, scale: ppp });
        }
    }

    /// Returns the top-level blocks whose diagram size changed.
    fn receive_mermaid(&mut self, ctx: &egui::Context) -> Vec<usize> {
        let g = self.gen_.load(Ordering::Relaxed);
        let mut changed = Vec::new();
        for (dg, id, r, scale) in self.done.try_iter() {
            if dg != g {
                continue;
            }
            let slot = &mut self.slots[id];
            let old = slot.tex.as_ref().map(|t| t.1);
            match r {
                Ok(img) => {
                    let size = Vec2::new(img.size[0] as f32, img.size[1] as f32) / scale;
                    let tex = ctx.load_texture(format!("mermaid{id}"), img, TextureOptions::LINEAR);
                    slot.tex = Some((tex, size));
                    slot.err = None;
                }
                Err(e) => {
                    slot.tex = None;
                    slot.err = Some(e);
                }
            }
            if slot.tex.as_ref().map(|t| t.1) != old || slot.tex.is_none() {
                changed.push(self.mermaid_owner[id]);
            }
        }
        changed
    }

    fn relayout(&mut self, ctx: &egui::Context, only: Option<&[usize]>) {
        let Some(key) = self.key else { return };
        let colors = Colors::new(key.dark);
        let w = self.col_w;
        let slots = &self.slots;
        let blocks = &self.blocks;
        let starts = &self.mermaid_start;
        let laid = &mut self.laid;
        ctx.fonts_mut(|f| {
            let mut lay = |i: usize| {
                let mut l = Lay { f, c: &colors, slots, next_mermaid: starts[i], items: Vec::new() };
                let h = l.block(&blocks[i], 0.0, 0.0, w);
                Laid { y: 0.0, h, items: l.items }
            };
            match only {
                Some(idx) => idx.iter().for_each(|&i| laid[i] = lay(i)),
                None => *laid = (0..blocks.len()).map(&mut lay).collect(),
            }
        });
        let mut y = PAD;
        for l in &mut self.laid {
            l.y = y;
            y += l.h + GAP;
        }
        self.total_h = y + PAD;
        // Restore the anchor so re-flow never jumps the reading position.
        if let Some(l) = self.laid.get(self.anchor.0) {
            self.pending_offset = Some(l.y + self.anchor.1 * l.h);
        }
    }

    fn update_anchor(&mut self) {
        let i = self.laid.partition_point(|l| l.y + l.h + GAP <= self.offset);
        if let Some(l) = self.laid.get(i) {
            self.anchor = (i, ((self.offset - l.y) / l.h.max(1.0)).clamp(0.0, 1.0));
        }
    }
}

impl eframe::App for Viewer {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
        if let Some(t0) = self.bench {
            self.frames += 1;
            if self.frames == 1 {
                eprintln!("bench: first frame {:.1} ms", t0.elapsed().as_secs_f64() * 1e3);
            }
            let pending = self.slots.iter().any(|s| s.tex.is_none() && s.err.is_none());
            if !pending {
                eprintln!("bench: diagrams ready {:.1} ms ({} frames)", t0.elapsed().as_secs_f64() * 1e3, self.frames);
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

impl Viewer {
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        for a in ctx.input(keys::actions) {
            self.apply(&ctx, a);
        }
        let zd = ctx.input(|i| i.zoom_delta());
        if zd != 1.0 {
            self.set_zoom(&ctx, self.zoom * zd);
        }

        let dark = ctx.theme() == egui::Theme::Dark;
        let ppp = ctx.pixels_per_point();
        let colors = Colors::new(dark);
        self.sync_mermaid(dark, ppp);

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Colors::bg(dark)))
            .show(ui, |ui| {
                let avail = ui.available_width();
                let col_w = if self.full_width { avail - 2.0 * PAD } else { READING_WIDTH.min(avail - 2.0 * PAD) }
                    .max(100.0);
                let key = Key { col_w: col_w.round() as u32, ppp: ppp.to_bits(), dark };
                if self.key != Some(key) {
                    self.key = Some(key);
                    self.col_w = col_w.round();
                    self.relayout(&ctx, None);
                }
                let changed = self.receive_mermaid(&ctx);
                if !changed.is_empty() {
                    self.relayout(&ctx, Some(&changed));
                }

                let mut sa = egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .content_margin(0.0)
                    .scroll_source(ScrollSource { drag: ScrollSource::NONE.drag, ..ScrollSource::ALL });
                if let Some(o) = self.pending_offset.take() {
                    sa = sa.vertical_scroll_offset(o);
                }
                let out = sa.show_viewport(ui, |ui, viewport| {
                    ui.set_min_height(self.total_h);
                    let origin = ui.max_rect().min + Vec2::new(((avail - self.col_w) / 2.0).max(PAD), 0.0);
                    let first = self.laid.partition_point(|l| l.y + l.h < viewport.min.y);
                    for (bi, l) in self.laid.iter().enumerate().skip(first) {
                        if l.y > viewport.max.y {
                            break;
                        }
                        let o = origin + Vec2::new(0.0, l.y);
                        for (ii, item) in l.items.iter().enumerate() {
                            paint(ui, item, o, ui.scope_id().with((bi, ii)), &colors, &self.slots);
                        }
                    }
                });
                self.offset = out.state.clamped_offset().y;
                self.view_h = out.inner_rect.height();
                self.update_anchor();
            });
    }
}

fn paint(ui: &mut egui::Ui, item: &Item, o: Pos2, id: egui::Id, c: &Colors, slots: &[Slot]) {
    match item {
        Item::Rect { rect, fill, stroke } => {
            ui.painter().rect(rect.translate(o.to_vec2()), CornerRadius::same(3), *fill, *stroke, egui::StrokeKind::Inside);
        }
        Item::Text { pos, galley, links } => text(ui, o + *pos, galley, id, links, c.text),
        Item::Code { rect, galley } => {
            let r = rect.translate(o.to_vec2());
            ui.painter().rect_filled(r, CornerRadius::same(6), c.code_bg);
            let inner = r.shrink(12.0);
            if galley.size().x <= inner.width() + 0.5 {
                text(ui, inner.min, galley, id, &[], c.text);
            } else {
                ui.scope_builder(egui::UiBuilder::new().max_rect(inner).id_salt(id), |ui| {
                    egui::ScrollArea::horizontal().id_salt(id).auto_shrink([false, true]).show(ui, |ui| {
                        let (r, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
                        text(ui, r.min, galley, id.with("c"), &[], c.text);
                    });
                });
            }
        }
        Item::Mermaid { rect, id: m } => {
            let r = rect.translate(o.to_vec2());
            match &slots[*m].tex {
                Some((tex, _)) => {
                    ui.painter().image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                }
                None => {
                    ui.painter().rect(r, CornerRadius::same(6), c.code_bg, Stroke::new(1.0, c.border), egui::StrokeKind::Inside);
                    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "rendering diagram…", FontId::proportional(13.0), c.weak);
                }
            }
        }
    }
}

/// Selectable text with clickable links.
fn text(ui: &mut egui::Ui, pos: Pos2, galley: &Arc<Galley>, id: egui::Id, links: &[(Range<usize>, String)], color: Color32) {
    let rect = Rect::from_min_size(pos, galley.size());
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    LabelSelectionState::label_text_selection(ui, &resp, pos, galley.clone(), color, Stroke::NONE);
    if links.is_empty() {
        return;
    }
    let Some(p) = resp.hover_pos() else { return };
    let ci = galley.cursor_from_pos(p - pos).index.0;
    // cursor_from_pos rounds to the nearest gap; check the char to either side.
    let hit = links.iter().find(|(r, _)| r.contains(&ci) || (ci > 0 && r.contains(&(ci - 1)) && r.end == ci));
    if let Some((_, url)) = hit {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        let clicked = resp.clicked();
        resp.on_hover_text_at_pointer(url.as_str());
        if clicked && (url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:")) {
            let url = url.clone();
            std::thread::spawn(move || {
                let _ = std::process::Command::new("xdg-open").arg(url).status();
            });
        }
    }
}

// ---------- layout ----------

struct Lay<'a, 'f> {
    f: &'a mut egui::epaint::text::FontsView<'f>,
    c: &'a Colors,
    slots: &'a [Slot],
    next_mermaid: usize,
    items: Vec<Item>,
}

impl Lay<'_, '_> {
    fn blocks(&mut self, bs: &[Block], x: f32, mut y: f32, w: f32) -> f32 {
        for (i, b) in bs.iter().enumerate() {
            if i > 0 {
                y += GAP;
            }
            y = self.block(b, x, y, w);
        }
        y
    }

    /// Lays out `b` with top-left (x, y) and width `w`; returns the bottom y.
    fn block(&mut self, b: &Block, x: f32, y: f32, w: f32) -> f32 {
        match b {
            Block::Heading { level, spans } => {
                let size = BODY * [2.0, 1.6, 1.35, 1.15, 1.0, 0.9][(*level as usize).clamp(1, 6) - 1];
                let y = y + if *level <= 2 { 8.0 } else { 4.0 };
                let mut bottom = self.spans(spans, x, y, w, size, true, self.c.text);
                if *level <= 2 {
                    bottom += 6.0;
                    self.items.push(Item::Rect {
                        rect: Rect::from_min_size(Pos2::new(x, bottom), Vec2::new(w, 1.0)),
                        fill: self.c.border,
                        stroke: Stroke::NONE,
                    });
                    bottom += 1.0;
                }
                bottom
            }
            Block::Paragraph(spans) => self.spans(spans, x, y, w, BODY, false, self.c.text),
            Block::Code { lang, text } => self.code(text, lang.as_deref(), x, y, w),
            Block::Mermaid(src) => {
                let id = self.next_mermaid;
                self.next_mermaid += 1;
                let slot = &self.slots[id];
                if let Some(err) = &slot.err {
                    let y = self.code(src, None, x, y, w);
                    return self.plain(&format!("mermaid: {err}"), x, y + 4.0, w, 13.0, self.c.err);
                }
                let size = match &slot.tex {
                    Some((_, s)) if s.x > w => Vec2::new(w, s.y * w / s.x),
                    Some((_, s)) => *s,
                    None => Vec2::new(w, 150.0),
                };
                self.items.push(Item::Mermaid { rect: Rect::from_min_size(Pos2::new(x, y), size), id });
                y + size.y
            }
            Block::Quote(bs) => {
                let bottom = self.blocks(bs, x + 16.0, y, w - 16.0);
                self.items.push(Item::Rect {
                    rect: Rect::from_min_max(Pos2::new(x, y), Pos2::new(x + 3.0, bottom)),
                    fill: self.c.border,
                    stroke: Stroke::NONE,
                });
                bottom
            }
            Block::List { start, items } => {
                let indent = 1.75 * BODY;
                let mut y = y;
                for (n, it) in items.iter().enumerate() {
                    if n > 0 {
                        y += 4.0;
                    }
                    match (it.checked, start) {
                        (Some(done), _) => {
                            let r = Rect::from_min_size(Pos2::new(x + 4.0, y + 5.0), Vec2::splat(13.0));
                            let fill = if done { self.c.link } else { Color32::TRANSPARENT };
                            self.items.push(Item::Rect { rect: r, fill, stroke: Stroke::new(1.0, self.c.weak) });
                        }
                        (None, Some(s)) => {
                            self.plain(&format!("{}.", s + n as u64), x, y, indent, BODY, self.c.weak);
                        }
                        (None, None) => {
                            self.plain("•", x + 6.0, y, indent, BODY, self.c.weak);
                        }
                    }
                    y = self.blocks(&it.blocks, x + indent, y, w - indent);
                }
                y
            }
            Block::Table { align, head, rows } => self.table(align, head, rows, x, y, w),
            Block::Rule => {
                let y = y + 8.0;
                self.items.push(Item::Rect {
                    rect: Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, 2.0)),
                    fill: self.c.border,
                    stroke: Stroke::NONE,
                });
                y + 10.0
            }
            Block::Footnote { label, blocks } => {
                self.plain(&format!("{label}."), x, y, 2.0 * BODY, 13.0, self.c.weak);
                self.blocks(blocks, x + 2.0 * BODY, y, w - 2.0 * BODY)
            }
        }
    }

    fn push_text(&mut self, x: f32, y: f32, job: LayoutJob, links: Vec<(Range<usize>, String)>) -> f32 {
        let galley = self.f.layout_job(job);
        let h = galley.size().y;
        self.items.push(Item::Text { pos: Vec2::new(x, y), galley, links });
        y + h
    }

    fn plain(&mut self, s: &str, x: f32, y: f32, w: f32, size: f32, color: Color32) -> f32 {
        let mut job = LayoutJob::single_section(s.to_owned(), fmt(size, false, color));
        job.wrap.max_width = w;
        self.push_text(x, y, job, Vec::new())
    }

    fn spans(&mut self, spans: &[Span], x: f32, y: f32, w: f32, size: f32, bold: bool, color: Color32) -> f32 {
        let (job, links) = spans_job(spans, size, bold, color, w, self.c);
        self.push_text(x, y, job, links)
    }

    fn code(&mut self, text: &str, lang: Option<&str>, x: f32, y: f32, w: f32) -> f32 {
        let mut job = crate::highlight::highlight(text, lang, self.c.dark, FontId::monospace(14.0));
        job.wrap.max_width = f32::INFINITY;
        for s in &mut job.sections {
            s.format.line_height = Some(14.0 * 1.45);
        }
        let galley = self.f.layout_job(job);
        let h = galley.size().y + 24.0;
        self.items.push(Item::Code { rect: Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h)), galley });
        y + h
    }

    fn table(&mut self, align: &[Align], head: &[Vec<Span>], rows: &[Vec<Vec<Span>>], x: f32, y: f32, w: f32) -> f32 {
        let ncol = head.len().max(rows.iter().map(Vec::len).max().unwrap_or(0));
        if ncol == 0 {
            return y;
        }
        let all_rows = std::iter::once((head, true)).chain(rows.iter().map(|r| (r.as_slice(), false)));
        // natural = unwrapped width, min = widest word (ponytail: words measured unstyled, fine for typical tables)
        let mut natural = vec![0.0f32; ncol];
        let mut min = vec![0.0f32; ncol];
        for (row, is_head) in all_rows.clone() {
            for (ci, cell) in row.iter().enumerate() {
                let (job, _) = spans_job(cell, BODY, is_head, self.c.text, f32::INFINITY, self.c);
                natural[ci] = natural[ci].max(self.f.layout_job(job).size().x + 2.0 * CELL_PAD);
                let s: String = cell.iter().map(|s| s.text.as_str()).collect();
                for word in s.split_whitespace() {
                    let job = LayoutJob::single_section(word.to_owned(), fmt(BODY, is_head, self.c.text));
                    min[ci] = min[ci].max(self.f.layout_job(job).size().x + 2.0 * CELL_PAD);
                }
            }
        }
        let widths = crate::table::column_widths(&natural, &min, w);
        let mut ty = y;
        for (row, is_head) in all_rows {
            let mut galleys = Vec::new();
            let mut row_h: f32 = 0.0;
            for (ci, cw) in widths.iter().enumerate() {
                let cell = row.get(ci).map(Vec::as_slice).unwrap_or(&[]);
                let (job, links) = spans_job(cell, BODY, is_head, self.c.text, cw - 2.0 * CELL_PAD, self.c);
                let g = self.f.layout_job(job);
                row_h = row_h.max(g.size().y + 2.0 * CELL_PAD);
                galleys.push((g, links));
            }
            let mut cx = x;
            if is_head {
                self.items.push(Item::Rect {
                    rect: Rect::from_min_size(Pos2::new(x, ty), Vec2::new(widths.iter().sum(), row_h)),
                    fill: self.c.head_bg,
                    stroke: Stroke::NONE,
                });
            }
            for (ci, ((g, links), cw)) in galleys.into_iter().zip(&widths).enumerate() {
                let cell = Rect::from_min_size(Pos2::new(cx, ty), Vec2::new(*cw, row_h));
                self.items.push(Item::Rect { rect: cell, fill: Color32::TRANSPARENT, stroke: Stroke::new(1.0, self.c.border) });
                let slack = cw - 2.0 * CELL_PAD - g.size().x;
                let dx = match align.get(ci) {
                    Some(Align::Center) => slack / 2.0,
                    Some(Align::Right) => slack,
                    _ => 0.0,
                }
                .max(0.0);
                self.items.push(Item::Text { pos: Vec2::new(cx + CELL_PAD + dx, ty + CELL_PAD), galley: g, links });
                cx += cw;
            }
            ty += row_h;
        }
        ty
    }
}

fn fmt(size: f32, bold_: bool, color: Color32) -> TextFormat {
    TextFormat {
        font_id: FontId::new(size, if bold_ { bold() } else { FontFamily::Proportional }),
        line_height: Some(size * 1.5),
        color,
        ..Default::default()
    }
}

fn spans_job(spans: &[Span], size: f32, bold_: bool, color: Color32, w: f32, c: &Colors) -> (LayoutJob, Vec<(Range<usize>, String)>) {
    let mut job = LayoutJob::default();
    job.wrap.max_width = w;
    let mut links = Vec::new();
    let mut chars = 0;
    for s in spans {
        let mut f = fmt(size, bold_ || s.style.bold, color);
        f.italics = s.style.italic;
        if s.style.strike {
            f.strikethrough = Stroke::new(1.0, color);
        }
        if s.style.code {
            f.font_id = FontId::monospace(size * 0.875);
            f.background = c.code_bg;
        }
        if let Some(url) = &s.link {
            f.color = c.link;
            f.underline = Stroke::new(1.0, c.link);
            let n = s.text.chars().count();
            links.push((chars..chars + n, url.clone()));
        }
        chars += s.text.chars().count();
        job.append(&s.text, 0.0, f);
    }
    if job.text.is_empty() {
        // keep empty cells/paragraphs one line tall
        job.append(" ", 0.0, fmt(size, false, color));
    }
    (job, links)
}

// ---------- fonts ----------

/// System sans (regular + bold) and mono via fontdb, egui defaults as fallback (emoji, symbols).
/// ponytail: fontdb family guesses, not fontconfig rules; switch to fc-match if users complain.
fn install_fonts(ctx: &egui::Context) {
    use resvg::usvg::fontdb::{Database, Family, Query, Weight};
    let mut db = Database::new();
    db.load_system_fonts();
    let pick = |names: &[&str], generic: Family, weight: Weight| -> Option<Vec<u8>> {
        let fams: Vec<Family> = names.iter().map(|n| Family::Name(n)).chain([generic]).collect();
        let id = db.query(&Query { families: &fams, weight, ..Default::default() })?;
        db.with_face_data(id, |data, _| data.to_vec())
    };
    let sans = ["Inter", "Noto Sans", "DejaVu Sans", "Liberation Sans", "Ubuntu"];
    let mono = ["JetBrains Mono", "Noto Sans Mono", "DejaVu Sans Mono", "Liberation Mono", "Ubuntu Mono"];
    let mut defs = egui::FontDefinitions::default();
    let add = |defs: &mut egui::FontDefinitions, name: &str, data: Option<Vec<u8>>, family: FontFamily| {
        if let Some(d) = data {
            defs.font_data.insert(name.into(), Arc::new(egui::FontData::from_owned(d)));
            defs.families.entry(family).or_default().insert(0, name.into());
        }
    };
    add(&mut defs, "sans", pick(&sans, Family::SansSerif, Weight::NORMAL), FontFamily::Proportional);
    add(&mut defs, "mono", pick(&mono, Family::Monospace, Weight::NORMAL), FontFamily::Monospace);
    let prop = defs.families[&FontFamily::Proportional].clone();
    defs.families.insert(bold(), prop);
    add(&mut defs, "sans-bold", pick(&sans, Family::SansSerif, Weight::BOLD), bold());
    ctx.set_fonts(defs);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::epaint::Shape;

    fn frame(ctx: &egui::Context, v: &mut Viewer, w: f32, h: f32) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))),
            ..Default::default()
        };
        ctx.run_ui(input, |ui| v.show(ui))
    }

    fn visible_content(out: &egui::FullOutput, screen: Rect) -> usize {
        fn count(s: &Shape, screen: Rect) -> usize {
            match s {
                Shape::Vec(v) => v.iter().map(|s| count(s, screen)).sum(),
                Shape::Text(t) if !t.galley.is_empty() => t.visual_bounding_rect().intersects(screen) as usize,
                Shape::Mesh(m) => m.calc_bounds().intersects(screen) as usize,
                _ => 0,
            }
        }
        out.shapes.iter().filter(|c| c.clip_rect.intersects(screen)).map(|c| count(&c.shape, screen)).sum()
    }

    fn big_doc() -> Vec<Block> {
        let sample = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/sample.md")).unwrap();
        crate::parse::parse(&sample.repeat(40))
    }

    /// PRD §8.1: every scroll position shows content — no white screen.
    #[test]
    fn no_blank_frames() {
        let ctx = egui::Context::default();
        let mut v = Viewer::new(&ctx, big_doc(), false, 1.0);
        let (w, h) = (900.0, 700.0);
        frame(&ctx, &mut v, w, h);
        frame(&ctx, &mut v, w, h);
        assert!(v.total_h > 10.0 * h, "doc should be long: {}", v.total_h);
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h));
        let mut off = 0.0;
        while off < v.total_h {
            v.pending_offset = Some(off);
            let out = frame(&ctx, &mut v, w, h);
            assert!(visible_content(&out, screen) > 0, "blank frame at offset {off} of {}", v.total_h);
            off += h / 3.0;
        }
    }

    /// PRD §5.3: re-flow (full width, resize) keeps the reading position.
    #[test]
    fn reflow_keeps_anchor() {
        let ctx = egui::Context::default();
        let mut v = Viewer::new(&ctx, big_doc(), false, 1.0);
        frame(&ctx, &mut v, 900.0, 700.0);
        v.pending_offset = Some(v.total_h / 2.0);
        frame(&ctx, &mut v, 900.0, 700.0);
        frame(&ctx, &mut v, 900.0, 700.0);
        let (block, _) = v.anchor;
        v.full_width = true;
        for _ in 0..3 {
            frame(&ctx, &mut v, 1600.0, 700.0);
        }
        assert_eq!(v.anchor.0, block, "anchor block changed after full-width + resize");
    }
}
