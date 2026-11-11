#![allow(dead_code)]

use crate::doc::{Align, Block, Cell, ListItem, Span, Style};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

enum Kind {
    Root,
    Quote,
    List(Option<u64>, Vec<ListItem>),
    Item(Option<bool>),
    Footnote(String),
    Table {
        align: Vec<Align>,
        head: Vec<Cell>,
        rows: Vec<Vec<Cell>>,
        row: Vec<Cell>,
    },
}

struct Frame {
    kind: Kind,
    blocks: Vec<Block>,
}

#[derive(Default)]
struct Inline {
    spans: Vec<Span>,
    bold: u32,
    italic: u32,
    strike: u32,
    image: u32,
    links: Vec<String>,
}

impl Inline {
    fn push(&mut self, text: &str, code: bool) {
        let style = Style {
            bold: self.bold > 0,
            italic: self.italic > 0 || self.image > 0,
            strike: self.strike > 0,
            code,
        };
        let link = if self.image > 0 {
            None
        } else {
            self.links.last().cloned()
        };
        self.push_span(text, style, link);
    }

    fn push_span(&mut self, text: &str, style: Style, link: Option<String>) {
        match self.spans.last_mut() {
            Some(s) if s.style == style && s.link == link => s.text.push_str(text),
            _ => self.spans.push(Span {
                text: text.to_string(),
                style,
                link,
            }),
        }
    }
}

pub fn parse(src: &str) -> Vec<Block> {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let mut stack = vec![Frame {
        kind: Kind::Root,
        blocks: vec![],
    }];
    let mut inl = Inline::default();
    // Raw-text block being collected: (lang, text). Code, HTML and frontmatter.
    let mut raw: Option<(Option<String>, String)> = None;

    // Turns pending loose inline content (tight list items) into a paragraph.
    fn flush(stack: &mut [Frame], inl: &mut Inline) {
        if !inl.spans.is_empty() {
            let spans = std::mem::take(&mut inl.spans);
            if let Some(top) = stack.last_mut() {
                top.blocks.push(Block::Paragraph(spans));
            }
        }
    }
    fn push_block(stack: &mut [Frame], b: Block) {
        if let Some(top) = stack.last_mut() {
            top.blocks.push(b);
        }
    }

    for ev in Parser::new_ext(src, opts) {
        match ev {
            Event::Start(tag) => match tag {
                Tag::Paragraph | Tag::Heading { .. } | Tag::HtmlBlock => {
                    flush(&mut stack, &mut inl);
                    if matches!(tag, Tag::HtmlBlock) {
                        raw = Some((None, String::new()));
                    }
                }
                Tag::CodeBlock(kind) => {
                    flush(&mut stack, &mut inl);
                    let lang = match kind {
                        CodeBlockKind::Fenced(info) => {
                            info.split_whitespace().next().map(str::to_string)
                        }
                        CodeBlockKind::Indented => None,
                    };
                    raw = Some((lang, String::new()));
                }
                Tag::MetadataBlock(_) => {
                    flush(&mut stack, &mut inl);
                    raw = Some((Some("yaml".into()), String::new()));
                }
                Tag::BlockQuote(_)
                | Tag::List(_)
                | Tag::Item
                | Tag::FootnoteDefinition(_)
                | Tag::Table(_) => {
                    flush(&mut stack, &mut inl);
                    let kind = match tag {
                        Tag::BlockQuote(_) => Kind::Quote,
                        Tag::List(start) => Kind::List(start, vec![]),
                        Tag::Item => Kind::Item(None),
                        Tag::FootnoteDefinition(l) => Kind::Footnote(l.to_string()),
                        Tag::Table(a) => Kind::Table {
                            align: a
                                .iter()
                                .map(|a| match a {
                                    Alignment::None => Align::None,
                                    Alignment::Left => Align::Left,
                                    Alignment::Center => Align::Center,
                                    Alignment::Right => Align::Right,
                                })
                                .collect(),
                            head: vec![],
                            rows: vec![],
                            row: vec![],
                        },
                        _ => unreachable!(),
                    };
                    stack.push(Frame {
                        kind,
                        blocks: vec![],
                    });
                }
                Tag::Emphasis => inl.italic += 1,
                Tag::Strong => inl.bold += 1,
                Tag::Strikethrough => inl.strike += 1,
                Tag::Link { dest_url, .. } => inl.links.push(dest_url.to_string()),
                Tag::Image { .. } => inl.image += 1,
                _ => {}
            },
            Event::End(end) => match end {
                TagEnd::Paragraph => {
                    let spans = std::mem::take(&mut inl.spans);
                    push_block(&mut stack, Block::Paragraph(spans));
                }
                TagEnd::Heading(level) => {
                    let spans = std::mem::take(&mut inl.spans);
                    push_block(
                        &mut stack,
                        Block::Heading {
                            level: level as u8,
                            spans,
                        },
                    );
                }
                TagEnd::CodeBlock | TagEnd::HtmlBlock | TagEnd::MetadataBlock(_) => {
                    if let Some((lang, mut text)) = raw.take() {
                        if text.ends_with('\n') {
                            text.pop();
                        }
                        let b = match lang.as_deref() {
                            Some("mermaid") => Block::Mermaid(text),
                            _ => Block::Code { lang, text },
                        };
                        push_block(&mut stack, b);
                    }
                }
                TagEnd::TableCell => {
                    let cell = std::mem::take(&mut inl.spans);
                    if let Some(Frame {
                        kind: Kind::Table { row, .. },
                        ..
                    }) = stack.last_mut()
                    {
                        row.push(cell);
                    }
                }
                TagEnd::TableHead | TagEnd::TableRow => {
                    if let Some(Frame {
                        kind:
                            Kind::Table {
                                head, rows, row, ..
                            },
                        ..
                    }) = stack.last_mut()
                    {
                        let r = std::mem::take(row);
                        if end == TagEnd::TableHead {
                            *head = r
                        } else {
                            rows.push(r)
                        }
                    }
                }
                TagEnd::BlockQuote(_)
                | TagEnd::List(_)
                | TagEnd::Item
                | TagEnd::FootnoteDefinition
                | TagEnd::Table => {
                    flush(&mut stack, &mut inl);
                    if stack.len() < 2 {
                        continue;
                    }
                    let Some(Frame { kind, blocks }) = stack.pop() else {
                        continue;
                    };
                    let block = match kind {
                        Kind::Item(checked) => {
                            if let Some(Frame {
                                kind: Kind::List(_, items),
                                ..
                            }) = stack.last_mut()
                            {
                                items.push(ListItem { checked, blocks });
                                continue;
                            }
                            Block::Quote(blocks) // note: orphan item, can't happen with balanced events
                        }
                        Kind::Quote => Block::Quote(blocks),
                        Kind::List(start, items) => Block::List { start, items },
                        Kind::Footnote(label) => Block::Footnote { label, blocks },
                        Kind::Table {
                            align, head, rows, ..
                        } => Block::Table { align, head, rows },
                        Kind::Root => continue,
                    };
                    push_block(&mut stack, block);
                }
                TagEnd::Emphasis => inl.italic = inl.italic.saturating_sub(1),
                TagEnd::Strong => inl.bold = inl.bold.saturating_sub(1),
                TagEnd::Strikethrough => inl.strike = inl.strike.saturating_sub(1),
                TagEnd::Link => {
                    inl.links.pop();
                }
                TagEnd::Image => inl.image = inl.image.saturating_sub(1),
                _ => {}
            },
            Event::Text(t) | Event::Html(t) if raw.is_some() => {
                if let Some((_, buf)) = raw.as_mut() {
                    buf.push_str(&t);
                }
            }
            Event::Text(t) => inl.push(&t, false),
            Event::Code(t) => inl.push(&t, true),
            Event::Html(t) | Event::InlineHtml(t) => inl.push_span(&t, Style::default(), None),
            Event::FootnoteReference(l) => {
                let style = Style {
                    bold: inl.bold > 0,
                    italic: inl.italic > 0,
                    strike: inl.strike > 0,
                    code: false,
                };
                inl.push_span(&format!("[{l}]"), style, Some(format!("#fn-{l}")));
            }
            Event::SoftBreak => inl.push(" ", false),
            Event::HardBreak => inl.push("\n", false),
            Event::Rule => {
                flush(&mut stack, &mut inl);
                push_block(&mut stack, Block::Rule);
            }
            Event::TaskListMarker(c) => {
                if let Some(Frame {
                    kind: Kind::Item(checked),
                    ..
                }) = stack.last_mut()
                {
                    *checked = Some(c);
                }
            }
            _ => {}
        }
    }
    flush(&mut stack, &mut inl);
    // Unwind anything left open (shouldn't happen; events are balanced).
    while stack.len() > 1 {
        let f = stack.pop().unwrap();
        push_block(&mut stack, Block::Quote(f.blocks));
    }
    stack.pop().map(|f| f.blocks).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sp(text: &str) -> Span {
        Span {
            text: text.into(),
            style: Style::default(),
            link: None,
        }
    }
    fn st(text: &str, f: impl FnOnce(&mut Style)) -> Span {
        let mut s = sp(text);
        f(&mut s.style);
        s
    }
    fn lk(mut s: Span, url: &str) -> Span {
        s.link = Some(url.into());
        s
    }
    fn p(spans: Vec<Span>) -> Block {
        Block::Paragraph(spans)
    }
    fn code(lang: Option<&str>, text: &str) -> Block {
        Block::Code {
            lang: lang.map(Into::into),
            text: text.into(),
        }
    }
    fn item(checked: Option<bool>, blocks: Vec<Block>) -> ListItem {
        ListItem { checked, blocks }
    }

    #[test]
    fn table_driven() {
        let cases: Vec<(&str, Vec<Block>)> = vec![
            ("", vec![]),
            (
                "# Hi *there*",
                vec![Block::Heading {
                    level: 1,
                    spans: vec![sp("Hi "), st("there", |s| s.italic = true)],
                }],
            ),
            (
                "### h3",
                vec![Block::Heading {
                    level: 3,
                    spans: vec![sp("h3")],
                }],
            ),
            ("a\nb", vec![p(vec![sp("a b")])]),
            ("a  \nb", vec![p(vec![sp("a\nb")])]),
            (
                "**b *bi*** ~~s~~ `c`",
                vec![p(vec![
                    st("b ", |s| s.bold = true),
                    st("bi", |s| {
                        s.bold = true;
                        s.italic = true
                    }),
                    sp(" "),
                    st("s", |s| s.strike = true),
                    sp(" "),
                    st("c", |s| s.code = true),
                ])],
            ),
            (
                "[x **y**](http://u) z",
                vec![p(vec![
                    lk(sp("x "), "http://u"),
                    lk(st("y", |s| s.bold = true), "http://u"),
                    sp(" z"),
                ])],
            ),
            (
                "<https://a.b>",
                vec![p(vec![lk(sp("https://a.b"), "https://a.b")])],
            ),
            (
                "![alt *t*](img.png)",
                vec![p(vec![st("alt t", |s| s.italic = true)])],
            ),
            (
                "[![alt](i.png)](http://u)",
                vec![p(vec![st("alt", |s| s.italic = true)])],
            ),
            ("a <b>x</b>", vec![p(vec![sp("a <b>x</b>")])]),
            ("<div>\nhi\n</div>\n", vec![code(None, "<div>\nhi\n</div>")]),
            (
                "---\ntitle: x\n---\nbody",
                vec![code(Some("yaml"), "title: x"), p(vec![sp("body")])],
            ),
            (
                "```rust ignore\nfn x() {}\n```",
                vec![code(Some("rust"), "fn x() {}")],
            ),
            ("```\nplain\n\n```", vec![code(None, "plain\n")]),
            ("    indented\n", vec![code(None, "indented")]),
            (
                "```mermaid\ngraph TD\nA-->B\n```",
                vec![Block::Mermaid("graph TD\nA-->B".into())],
            ),
            (
                "> q\n>\n> > qq",
                vec![Block::Quote(vec![
                    p(vec![sp("q")]),
                    Block::Quote(vec![p(vec![sp("qq")])]),
                ])],
            ),
            ("a\n\n---\n", vec![p(vec![sp("a")]), Block::Rule]),
            (
                "- a\n- b\n  - c",
                vec![Block::List {
                    start: None,
                    items: vec![
                        item(None, vec![p(vec![sp("a")])]),
                        item(
                            None,
                            vec![
                                p(vec![sp("b")]),
                                Block::List {
                                    start: None,
                                    items: vec![item(None, vec![p(vec![sp("c")])])],
                                },
                            ],
                        ),
                    ],
                }],
            ),
            (
                "3. x\n\n4. y",
                vec![Block::List {
                    start: Some(3),
                    items: vec![
                        item(None, vec![p(vec![sp("x")])]),
                        item(None, vec![p(vec![sp("y")])]),
                    ],
                }],
            ),
            (
                "- [x] done\n- [ ] todo",
                vec![Block::List {
                    start: None,
                    items: vec![
                        item(Some(true), vec![p(vec![sp("done")])]),
                        item(Some(false), vec![p(vec![sp("todo")])]),
                    ],
                }],
            ),
            (
                "| a | b | c |\n|:-|:-:|--:|\n| 1 | **2** | |",
                vec![Block::Table {
                    align: vec![Align::Left, Align::Center, Align::Right],
                    head: vec![vec![sp("a")], vec![sp("b")], vec![sp("c")]],
                    rows: vec![vec![
                        vec![sp("1")],
                        vec![st("2", |s| s.bold = true)],
                        vec![],
                    ]],
                }],
            ),
            (
                "x[^n]\n\n[^n]: note",
                vec![
                    p(vec![sp("x"), lk(sp("[n]"), "#fn-n")]),
                    Block::Footnote {
                        label: "n".into(),
                        blocks: vec![p(vec![sp("note")])],
                    },
                ],
            ),
            ("$x$ \"q\" {#id}", vec![p(vec![sp("$x$ \"q\" {#id}")])]),
        ];
        for (src, want) in cases {
            assert_eq!(parse(src), want, "input: {src:?}");
        }
    }

    #[test]
    fn garbage_does_not_panic() {
        let inputs = [
            "",
            "```",
            "```rust\nunclosed",
            "~~~\n```\n",
            "> > > - > 1. - [ ] > ```\n",
            "- a\n  > b\n    - c\n      ```\n",
            "---\nunclosed: yaml",
            "---\n---",
            "| a |\n|---|\n| b | c | d |",
            "|\n|-|",
            "[^]: x\n[^a]: [^a]",
            "[^a]\n\n[^a]:\n> - | x |",
            "<div>\n- x\n",
            "![![![a](b)](c)](d)",
            "*_**~~`x",
            "\0\u{feff}\r\n\r\n\t- \t1)",
            "1. \n2.\n- [x]\n  - [ ]",
        ];
        for s in inputs {
            let _ = parse(s);
            let _ = parse(&s.repeat(20));
        }
        let all: String = inputs.concat();
        let _ = parse(&all);
        let nested: String = (0..500)
            .map(|i| if i % 2 == 0 { "> " } else { "- " })
            .collect();
        let _ = parse(&nested);
    }
}
