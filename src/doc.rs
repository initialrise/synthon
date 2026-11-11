//! Shared document model. Parser produces it, view consumes it.

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading { level: u8, spans: Vec<Span> },
    Paragraph(Vec<Span>),
    /// Frontmatter → lang "yaml"; raw block HTML → lang None.
    Code { lang: Option<String>, text: String },
    Mermaid(String),
    Quote(Vec<Block>),
    List { start: Option<u64>, items: Vec<ListItem> },
    Table { align: Vec<Align>, head: Vec<Cell>, rows: Vec<Vec<Cell>> },
    Rule,
    Footnote { label: String, blocks: Vec<Block> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

pub type Cell = Vec<Span>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: Style,
    pub link: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
}
