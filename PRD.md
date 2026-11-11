# synthon — PRD

A fast, low-memory, read-only Markdown viewer for Linux. Glow-level simplicity, but in a native GUI window so mermaid diagrams and highlighted code render as real graphics.

```
synthon README.md
cat notes.md | synthon -
```

## 1. Why

[md-viewer](https://github.com/aydiler/md-viewer) is the current tool. Problems:

- **White screen mid-scroll.** The document goes blank while reading. It probably comes from its viewport-virtualization code.
- **Tables ignore full width.** Turning on full width does not widen tables.
- **Too much scope.** It has tabs, a file explorer, an outline, search, LaTeX, a font picker, and colour settings. None of these are wanted.

synthon is a rewrite with a small scope. It reuses crates that md-viewer proved out (pulldown-cmark, syntect, merman), not its architecture.

## 2. Goals

1. Open one Markdown file, render it correctly, scroll it smoothly. Nothing else.
2. Never show a blank or white frame.
3. Tables follow the content width, including in full-width mode.
4. Meet the performance budgets in §6.

## 3. Non-goals (v1)

Tabs, file explorer, outline/TOC, in-document search, back/forward history, following relative `.md` links, live reload, LaTeX math, images, raw HTML rendering, settings files, font picker, custom colours, macOS/Windows, installers or distro packages.

## 4. Users and usage

One user (the author) reads docs, READMEs, and notes from the terminal. They launch, read, and quit with `q`. Docs are usually under 1 MB.

## 5. Functional requirements

### 5.1 CLI

```
synthon [--dark | --light] [--full-width] [--zoom <percent>] <FILE | ->
```

| Case | Behaviour |
|---|---|
| `FILE` exists | Open a window titled `<filename> — synthon` |
| `-` | Read stdin to EOF, title `stdin — synthon` |
| File missing or unreadable | Print the error to stderr, exit 1, no window |
| Invalid UTF-8 | Decode lossily (U+FFFD), still render |
| No theme flag | Follow the OS dark/light preference, fall back to light |
| `--zoom` | 50–300, default 100 |

No config file and no saved state. Every launch uses the defaults plus the CLI flags.

### 5.2 Rendering

CommonMark + GFM via `pulldown-cmark`: headings, paragraphs, emphasis, strikethrough, inline code, code blocks, block quotes, ordered/unordered/task lists, tables, horizontal rules, autolinks, footnotes.

| Element | Behaviour |
|---|---|
| Fenced code with a language | Syntax highlighted (`syntect`, pure-Rust regex backend); highlight theme follows the app theme |
| Code with an unknown or missing language | Monospace, unhighlighted |
| ` ```mermaid ` | Rendered as a diagram (`merman` → SVG → `resvg` → texture), scaled down to fit the content width, never upscaled past natural size |
| Mermaid parse error or unsupported type | Show the source as a code block plus a one-line error note. Never blank, never crash |
| Tables | Fill the available content width. Columns are sized in proportion to content; long cell text wraps. No horizontal scroll. Header row is bold, cells have borders |
| YAML frontmatter (leading `---` … `---`) | Shown as a `yaml` code block |
| Raw HTML (block or inline) | Shown as literal text |
| Images | Alt text shown in italics, with no loading |
| Wide code blocks | Horizontal scroll inside the block only; the page never scrolls sideways |

### 5.3 Layout

- **Reading mode (default):** content column of about 80ch, centred.
- **Full-width mode:** the content column fills the window minus padding. Every block re-flows to the new width, including tables and mermaid diagrams.
- Window resizes re-flow everything. Scroll position is anchored to the block at the top of the viewport, so it doesn't jump.

### 5.4 Interaction

| Key | Action |
|---|---|
| `j` / `↓` | Scroll down one line |
| `k` / `↑` | Scroll up one line |
| `d` / `Ctrl+D` | Half page down |
| `u` / `Ctrl+U` | Half page up |
| `PgDn` / `PgUp` | Full page |
| `g` / `Home` | Top |
| `G` / `End` | Bottom |
| `t` | Toggle dark/light |
| `f` | Toggle full width |
| `Ctrl+=` / `Ctrl+-` / `Ctrl+0` | Zoom in / out / reset (also `Ctrl+scroll`) |
| `q` / `Esc` | Quit |

Mouse:
- Wheel/touchpad scrolling.
- **Text selection:** click-drag selects text across paragraphs, lists, tables, and code blocks. `Ctrl+C` copies it as plain text.
- **Links:** `http(s)://` and `mailto:` open through `xdg-open`. Hovering shows the URL. Other links (relative, `#anchor`) do nothing in v1.

## 6. Performance budgets

Reference doc: 1 MB Markdown containing 10 mermaid diagrams and 50 code blocks. Machine: the author's Linux workstation, release build.

| Metric | Budget |
|---|---|
| Launch → first fully rendered frame | **< 100 ms** |
| Peak RSS | **< 80 MB** |
| Scroll | 60 fps, no dropped frames during continuous `j` hold or fling |
| Toggle full width / theme / zoom | Next frame (< 16 ms) after the first re-layout at that width |
| 10 MB doc | Must work without blank frames. Budgets do not apply |

## 7. Architecture (decided)

- **GUI:** `eframe`/`egui` (wgpu or glow backend, Wayland + X11). egui was chosen over iced because it supports text selection across widgets.
- **Pipeline:** `parse → Vec<Block>` once at load. On each width/zoom/theme change, lay out every block once into cached galleys/textures with known heights. Each frame paints only the blocks that intersect the viewport, using the cached heights.
- **No virtualization tricks.** The whole document is laid out up front, so every block's height is known and no frame shows unlaid content. This is the direct fix for the white screen. Heavier virtualization is added only if a real doc misses the budget.
- **Mermaid** renders on a worker thread (the S0 spike measured ~10 ms per diagram) and is cached as a texture per (width, zoom, theme). A placeholder is shown until the diagram is ready.
- **Distribution:** `cargo install synthon`, a single binary. Syntaxes and themes are embedded.

## 8. Acceptance tests

1. **No white screen:** scroll the reference doc top→bottom→top with `j`/`k` held, with `G`/`g`, and with a fast touchpad fling. Every frame shows content (automated: render offscreen at N scroll offsets and assert that no frame is entirely background colour).
2. **Tables:** a 10-column table with long cells fits the column in reading mode, gets wider in full-width mode, and never scrolls horizontally.
3. **Mermaid:** a valid flowchart and sequence diagram render. An invalid block shows its source plus an error note.
4. **Selection:** select from the middle of a paragraph into a table, copy, and paste the matching plain text.
5. **CLI:** a missing file gives exit 1 and a stderr message; `-` reads stdin; `--dark --full-width --zoom 150` take effect on the first frame.
6. **Budgets:** §6 measured with `/usr/bin/time -v` and a first-frame timestamp log.

## 9. Future (not v1)

macOS + Windows, live reload, relative `.md` link navigation, `#anchor` jumps, local images, find-in-doc, AUR/.deb packaging.

## 10. Open questions

- ~~How much mermaid syntax does `merman` cover?~~ Resolved by the S0 spike: GO, covers all common types. Rendering is ~10 ms per diagram, so it runs on a worker thread.
- Should `Esc` clear an active text selection before it quits? Default for now: it quits immediately.
