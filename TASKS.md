# synthon — Work breakdown

Each chunk owns **one file**, so parallel work never touches the same file. Chunks talk only through the contracts in Wave 0. The PRD is the spec; section numbers (§) refer to `PRD.md`.

```
Wave 0 (serial)      W0 scaffold + contracts          S0 merman spike (go/no-go)
                              │                              │
Wave 1 (parallel)    A cli   B parse   C highlight   D mermaid   E table   K keys   F view*   G harness
                              │
Wave 2 (serial)      I integrate → acceptance tests → budget tuning
```
`*` F builds against hand-written `Vec<Block>` fixtures until B lands.

---

## Wave 0 — must finish first

### W0 · Scaffold + contracts  (`Cargo.toml`, `src/main.rs` stubs, `src/doc.rs`)
Create the crate, add the deps, and add every module below with real signatures and `todo!()` bodies. Fill in `doc.rs` completely. It is the shared model:

```rust
// src/doc.rs
pub enum Block {
    Heading { level: u8, spans: Vec<Span> },
    Paragraph(Vec<Span>),
    Code { lang: Option<String>, text: String },   // frontmatter → lang "yaml"; block HTML → lang None
    Mermaid(String),
    Quote(Vec<Block>),
    List { start: Option<u64>, items: Vec<ListItem> },
    Table { align: Vec<Align>, head: Vec<Cell>, rows: Vec<Vec<Cell>> },
    Rule,
    Footnote { label: String, blocks: Vec<Block> },
}
pub struct ListItem { pub checked: Option<bool>, pub blocks: Vec<Block> }
pub type Cell = Vec<Span>;
pub enum Align { None, Left, Center, Right }
pub struct Span { pub text: String, pub style: Style, pub link: Option<String> }
#[derive(Default, Clone, Copy)]
pub struct Style { pub bold: bool, pub italic: bool, pub strike: bool, pub code: bool }
```

Module signatures to stub:
```rust
parse::parse(src: &str) -> Vec<Block>
highlight::highlight(code: &str, lang: Option<&str>, dark: bool, font: egui::FontId) -> egui::text::LayoutJob
mermaid::render(src: &str, dark: bool, max_width_px: u32, scale: f32) -> Result<egui::ColorImage, String>
table::column_widths(natural: &[f32], min: &[f32], available: f32) -> Vec<f32>
keys::actions(input: &egui::InputState) -> Vec<Action>      // Action enum defined in keys.rs
cli::Args::parse() + cli::read_input(&Args) -> Result<(String /*title*/, String /*text*/), String>
```
**Done:** `cargo check` passes. Deps: `eframe`, `pulldown-cmark`, `syntect` (fancy-regex, no onig), `merman`, `resvg`, `clap` (derive).

### S0 · Mermaid spike  (throwaway, `spike/`) — runs alongside W0
Feed `merman` the author's real diagrams (flowchart, sequence, plus any others in use). Measure render time per diagram.
**Done:** a go/no-go note. Go = it covers the diagrams and renders 10 in < 50 ms total. No-go = D switches to shelling out to `mmdc`, and §10 of the PRD is updated.

**Result (2026-11-11): GO.** merman `0.10.0-alpha.2` (features `all-diagrams, svg, layout-cytoscape`, no ELK) with `SvgPipeline::resvg_safe()` renders flowchart, sequence, class, state, ER, gantt, pie, and mindmap correctly in light and dark themes (`SourcePresentationConfig {"theme":"dark"}`). Each diagram takes 6–20 ms (SVG ~1–5 ms, the rest is resvg raster). The whole set of 10 takes ~100 ms including the system fontdb load, with 33 MB RSS. Parse errors and unknown types return a typed `Err`. **Consequence:** 10 diagrams take up the whole 100 ms launch budget, so mermaid renders on a worker thread from day one.

---

## Wave 1 — fully parallel after W0

| Chunk | File | Scope (PRD §) | Done when |
|---|---|---|---|
| **A · CLI + input** | `src/cli.rs` | §5.1: flags, file / `-` stdin, lossy UTF-8, stderr + exit 1, `--zoom` 50–300 range check | Unit tests: missing file → `Err`, invalid UTF-8 → U+FFFD, `-` reads stdin, bad zoom rejected |
| **B · Parser** | `src/parse.rs` | §5.2: pulldown-cmark GFM options → `Vec<Block>`; frontmatter → `Code{yaml}`; HTML → literal text; images → italic alt span; footnote refs → `[n]` span; ```` ```mermaid ```` → `Block::Mermaid` | Table-driven tests: one input string → expected `Vec<Block>` per element type |
| **C · Highlight** | `src/highlight.rs` | §5.2: embedded syntect syntaxes and themes, dark/light theme pair, unknown lang → plain mono | Tests: `rust` produces >1 colour; `"nope"` produces 1 colour; first call < 20 ms |
| **D · Mermaid** | `src/mermaid.rs` | §5.2: merman → SVG → resvg → `ColorImage`, scaled down to `max_width_px` and never upscaled; theme-aware | Tests: valid flowchart → `Ok` image ≤ max width; garbage → `Err(msg)` |
| **E · Table widths** | `src/table.rs` | §5.2: pure algorithm. If Σnatural ≤ available, scale up proportionally to fill; otherwise shrink the widest columns first, never below `min` | Tests: fits, overflow, all-min, single column; Σ output == available (when ≥ Σmin) |
| **K · Keys** | `src/keys.rs` | §5.4: input → `Action` enum (`ScrollLines(i32)`, `HalfPage(±)`, `Page(±)`, `Top`, `Bottom`, `ToggleTheme`, `ToggleFullWidth`, `Zoom(±/reset)`, `Quit`) | Tests: each key in the §5.4 table maps to the right action; `d` and `Ctrl+D` give the same action |
| **F · View** | `src/view.rs` | §5.3, §7: eframe app; layout every block once per (width, zoom, theme) into cached galleys/textures with heights; paint only the visible blocks; scroll anchor on resize; reading/full-width column; apply `Action`s; selectable labels; link hover + `xdg-open` | Runs on a hand-built fixture `Vec<Block>` that covers every variant; resize and toggles don't jump scroll |
| **G · Harness** | `tests/`, `fixtures/`, `bench.sh` | §6, §8: 1 MB reference doc (10 mermaid, 50 code, wide tables); offscreen render test asserting no all-background frame at N scroll offsets; `bench.sh` measuring first-frame ms + peak RSS | Harness runs against the stubs and fails for the right reasons |

F is the critical path and the largest chunk. Start it first. The others are about half a day each.

---

## Wave 2 — serial

### I · Integrate
Wire A → B → F in `main.rs`, and replace F's fixtures with C/D/E/K. Run acceptance tests 1–6 from §8.

### T · Tune
Run `bench.sh`. If launch is > 100 ms, move mermaid to a worker thread (the ponytail note in §7) before anything else. If RSS is > 80 MB, check syntect's loaded syntax set and the texture sizes first.

---

## Status (2026-11-11)

- ✅ W0, S0, A, B, C, D, E, K, F, G, I. 25 tests pass, clippy is clean.
- ✅ T (time): first frame 67 ms (tiny), 88 ms (1 MB). This uses progressive layout plus threads that overlap startup work with GL init.
- ⚠️ T (memory): 142 MB RSS, of which 58 MB is private. The rest is the Mesa driver and binary pages. Needs a budget decision (PRD §6).
- Not done: §8.4 (selection copy) and §8.2 (tables at 10 columns) are checked by eye only. Keyboard actions are unit-tested but haven't been driven in the real window.

## Assigning work
- Parallel agents/people: one chunk each, in its own git worktree or branch. Merges are clean because the files don't overlap.
- If W0's contracts need to change mid-wave, change `doc.rs` in one PR and rebase the others. Don't fork the types.
