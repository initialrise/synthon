#![allow(dead_code)]

use clap::Parser;
use std::io::Read;
use std::path::Path;

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

impl Args {
    pub fn validate(&self) -> Result<(), String> {
        if (50..=300).contains(&self.zoom) {
            Ok(())
        } else {
            Err(format!("synthon: --zoom must be 50–300, got {}", self.zoom))
        }
    }
}

/// Returns (window title, document text).
pub fn read_input(args: &Args) -> Result<(String, String), String> {
    let err = |e: std::io::Error| format!("synthon: {}: {e}", args.file);
    let (title, bytes) = if args.file == "-" {
        let mut buf = Vec::new();
        std::io::stdin().read_to_end(&mut buf).map_err(err)?;
        ("stdin".to_string(), buf)
    } else {
        let path = Path::new(&args.file);
        let title = path
            .file_name()
            .map_or(args.file.clone(), |n| n.to_string_lossy().into_owned());
        (title, std::fs::read(path).map_err(err)?)
    };
    Ok((title, String::from_utf8_lossy(&bytes).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(file: &str, zoom: u16) -> Args {
        Args {
            file: file.into(),
            dark: false,
            light: false,
            full_width: false,
            zoom,
        }
    }

    fn tmp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("synthon-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn missing_file_errs() {
        let e = read_input(&args("/nonexistent/nope.md", 100)).unwrap_err();
        assert!(e.starts_with("synthon: /nonexistent/nope.md: "), "{e}");
    }

    #[test]
    fn invalid_utf8_is_lossy() {
        let p = tmp("bad.md", b"ok \xff\xfe end");
        let (_, text) = read_input(&args(p.to_str().unwrap(), 100)).unwrap();
        assert!(text.contains('\u{FFFD}') && text.starts_with("ok "));
    }

    #[test]
    fn title_is_basename() {
        let p = tmp("notes.md", b"# hi");
        let (title, text) = read_input(&args(p.to_str().unwrap(), 100)).unwrap();
        assert_eq!((title.as_str(), text.as_str()), ("notes.md", "# hi"));
    }

    #[test]
    fn zoom_range() {
        assert!(args("x", 49).validate().is_err());
        assert!(args("x", 301).validate().is_err());
        assert!(args("x", 50).validate().is_ok());
        assert!(args("x", 300).validate().is_ok());
    }
}
