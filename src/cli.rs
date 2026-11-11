use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about = "Fast, read-only markdown viewer")]
pub struct Args {
    /// Markdown file, or `-` for stdin
    pub file: String,
    #[arg(long, conflicts_with = "light")]
    pub dark: bool,
    #[arg(long)]
    pub light: bool,
    #[arg(long)]
    pub full_width: bool,
    /// Zoom percent, 50–300
    #[arg(long, default_value_t = 100)]
    pub zoom: u16,
}

/// Returns (window title, document text).
pub fn read_input(_args: &Args) -> Result<(String, String), String> {
    todo!("A: read file or stdin")
}
