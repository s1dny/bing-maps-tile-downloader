use anyhow::{anyhow, Context, Result};
use clap::{ArgAction, Parser};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use walkdir::WalkDir;
use which::which;

#[derive(Parser, Debug)]
pub struct Args {
    /// Input directory to scan for .glb files
    #[arg(default_value = ".")]
    pub input_dir: PathBuf,

    /// Output directory (defaults to <input_dir>/processed)
    #[arg(short, long)]
    pub out: Option<PathBuf>,

    /// Recurse into subdirectories
    #[arg(short, long, action = ArgAction::SetTrue)]
    pub recursive: bool,

    /// Overwrite outputs if they already exist
    #[arg(short = 'f', long, action = ArgAction::SetTrue)]
    pub force: bool,

    /// Limit worker threads (default = number of logical CPUs)
    #[arg(short = 'j', long)]
    pub jobs: Option<usize>,

    /// Force using npx instead of a globally installed gltf-transform
    #[arg(long, action = ArgAction::SetTrue)]
    pub use_npx: bool,

    /// Dry run: list what would be processed without executing
    #[arg(long, action = ArgAction::SetTrue)]
    pub dry_run: bool,
}

enum CliKind {
    Global(PathBuf),         // e.g., /usr/local/bin/gltf-transform
    Npx { package: String }, // e.g., @gltf-transform/cli
}

pub fn run_decompress(args: Args) -> Result<()> {

    if let Some(n) = args.jobs {
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global()
            .ok(); // harmless if already set
    }

    // Decide output dir.
    let out_dir = args
        .out
        .clone()
        .unwrap_or_else(|| args.input_dir.join("processed"));
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("Failed to create output directory {:?}", out_dir))?;

    // Discover CLI.
    let cli = detect_cli(args.use_npx)?;

    // Gather .glb files.
    let files = collect_glb_files(&args.input_dir, args.recursive)?;
    println!(
        "Found {} .glb file(s) in {:?}. Output → {:?}",
        files.len(),
        args.input_dir,
        out_dir
    );
    if files.is_empty() {
        return Ok(());
    }

    // Progress bar.
    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} • {msg}",
        )
        .unwrap()
        .progress_chars("##-"),
    );

    let force = args.force;
    let dry_run = args.dry_run;

    // Process in parallel.
    let results: Vec<Result<()>> = files
        .par_iter()
        .map(|in_path| {
            let file_name = in_path
                .file_name()
                .ok_or_else(|| anyhow!("Bad filename"))?;
            let out_path = out_dir.join(file_name);

            if out_path.exists() && !force {
                pb.inc(1);
                pb.set_message(format!(
                    "{} (skipped, exists)",
                    file_name.to_string_lossy()
                ));
                return Ok(());
            }

            if dry_run {
                pb.inc(1);
                pb.set_message(format!("{} (dry-run)", file_name.to_string_lossy()));
                return Ok(());
            }

            fs::create_dir_all(&out_dir)
                .with_context(|| format!("Creating parent for {:?}", out_path))?;

            // Build command
            let status = match &cli {
                CliKind::Global(bin) => {
                    Command::new(bin)
                        .arg("ktxdecompress")
                        .arg(in_path)
                        .arg(&out_path)
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::piped())
                        .status()
                        .with_context(|| format!("Failed to spawn gltf-transform for {:?}", in_path))?
                }
                CliKind::Npx { package } => {
                    Command::new("npx")
                        .arg("-y")
                        .arg(package)
                        .arg("ktxdecompress")
                        .arg(in_path)
                        .arg(&out_path)
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::piped())
                        .status()
                        .with_context(|| format!("Failed to spawn npx for {:?}", in_path))?
                }
            };

            if !status.success() {
                return Err(anyhow!(
                    "ktxdecompress failed for {:?} (exit status {:?})",
                    in_path,
                    status.code()
                ));
            }

            pb.inc(1);
            pb.set_message(format!("{} (ok)", file_name.to_string_lossy()));
            Ok(())
        })
        .collect();

    pb.finish_and_clear();

    // Summarize errors if any.
    let mut failures = Vec::new();
    for res in results {
        if let Err(e) = res {
            failures.push(e);
        }
    }

    if failures.is_empty() {
        println!(
            "All done. Decompressed files are in: {}",
            out_dir.display()
        );
        Ok(())
    } else {
        eprintln!("Completed with {} error(s):", failures.len());
        for (i, e) in failures.iter().enumerate() {
            eprintln!("  {}. {:#}", i + 1, e);
        }
        Err(anyhow!("Some files failed. See errors above."))
    }
}

fn detect_cli(force_npx: bool) -> Result<CliKind> {
    if !force_npx {
        if let Ok(p) = which("gltf-transform") {
            return Ok(CliKind::Global(p));
        }
    }
    // Fallback: npx @gltf-transform/cli
    Ok(CliKind::Npx {
        package: "@gltf-transform/cli".to_string(),
    })
}

fn collect_glb_files(root: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();

    if recursive {
        for entry in WalkDir::new(root).follow_links(true) {
            let entry = entry?;
            if entry.file_type().is_file() && has_glb_ext(entry.path()) {
                out.push(entry.into_path());
            }
        }
    } else {
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && has_glb_ext(&path) {
                out.push(path);
            }
        }
    }

    out.sort();
    Ok(out)
}

fn has_glb_ext(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|ext| ext.eq_ignore_ascii_case("glb"))
        .unwrap_or(false)
}
