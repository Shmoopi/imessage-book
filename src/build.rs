//! Producing a PDF from the generated LaTeX.
//!
//! Prefers Tectonic (self-contained, auto-fetches packages, no MacTeX needed), then
//! falls back to a system `latexmk`/`xelatex` install.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// Pick the best available engine automatically.
    Auto,
    /// Force the Tectonic engine.
    Tectonic,
    /// Force a system TeX install (`latexmk`, then `xelatex`).
    System,
}

fn in_path(tool: &str) -> bool {
    Command::new("which")
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Write `latex` to `out_dir/book.tex`.
pub fn write_source(out_dir: &Path, latex: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("creating output dir {}", out_dir.display()))?;
    let tex = out_dir.join("book.tex");
    std::fs::write(&tex, latex).with_context(|| format!("writing {}", tex.display()))?;
    Ok(tex)
}

/// Compile `out_dir/book.tex` into `out_dir/book.pdf`.
pub fn build_pdf(out_dir: &Path, engine: Engine, open_pdf: bool) -> Result<PathBuf> {
    let use_tectonic = match engine {
        Engine::Tectonic => true,
        Engine::System => false,
        Engine::Auto => in_path("tectonic"),
    };

    if use_tectonic {
        if !in_path("tectonic") {
            anyhow::bail!(
                "Tectonic is not installed. Install it with `brew install tectonic`, \
                 or pass `--engine system` to use an existing TeX install."
            );
        }
        run(out_dir, "tectonic", &["book.tex"])?;
    } else {
        compile_system(out_dir)?;
    }

    let pdf = out_dir.join("book.pdf");
    if !pdf.exists() {
        anyhow::bail!(
            "the LaTeX engine finished but produced no PDF at {}",
            pdf.display()
        );
    }
    println!("Wrote {}", pdf.display());
    if open_pdf {
        if let Err(e) = open::that(&pdf) {
            eprintln!("(could not open PDF automatically: {e})");
        }
    }
    Ok(pdf)
}

fn compile_system(out_dir: &Path) -> Result<()> {
    if in_path("latexmk") {
        return run(
            out_dir,
            "latexmk",
            &["-xelatex", "-interaction=nonstopmode", "book.tex"],
        );
    }
    if in_path("xelatex") {
        // Run twice so the table of contents resolves.
        run(
            out_dir,
            "xelatex",
            &["-interaction=nonstopmode", "book.tex"],
        )?;
        return run(
            out_dir,
            "xelatex",
            &["-interaction=nonstopmode", "book.tex"],
        );
    }
    anyhow::bail!(
        "No LaTeX engine found. Install Tectonic (`brew install tectonic`, recommended) \
         or a TeX distribution providing `latexmk`/`xelatex`."
    )
}

fn run(dir: &Path, program: &str, args: &[&str]) -> Result<()> {
    println!("Running {program} {}", args.join(" "));
    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .with_context(|| format!("running {program}"))?;
    if !status.success() {
        anyhow::bail!(
            "{program} failed (exit {}). See the log in {} for details.",
            status.code().unwrap_or(-1),
            dir.display()
        );
    }
    Ok(())
}
