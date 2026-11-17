// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use skriv_fidelity::structural::Mode;

/// ODT round-trip fidelity harness for skriv, with LibreOffice as oracle.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Round-trip a corpus and write a report.
    Run {
        /// Corpus directories (searched recursively for .odt and .ott).
        #[arg(required = true)]
        corpus: Vec<PathBuf>,
        /// Report directory.
        #[arg(long, default_value = "fidelity/reports/latest")]
        out: PathBuf,
        /// Scratch directory for round-tripped files, PDFs and page images.
        #[arg(long, default_value = "fidelity/work/run")]
        work: PathBuf,
        /// Skip LibreOffice rendering (structural checks only).
        #[arg(long)]
        no_visual: bool,
        /// Skip the LibreOffice re-save comparison.
        #[arg(long)]
        no_lo_loop: bool,
        #[arg(long, env = "SKRIV_SOFFICE", default_value = "soffice")]
        soffice: PathBuf,
        /// Parallel LibreOffice processes.
        #[arg(long, default_value_t = 4)]
        workers: usize,
        #[arg(long, default_value_t = 50)]
        dpi: u32,
        /// Changed-pixel ratio per page above which a page counts as visibly different.
        #[arg(long, default_value_t = 0.001)]
        threshold: f64,
        /// Only the first N files (sorted by path).
        #[arg(long)]
        limit: Option<usize>,
        /// Fail (exit 1) if the kill criteria are not met.
        #[arg(long)]
        enforce: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Run {
            corpus,
            out,
            work,
            no_visual,
            no_lo_loop,
            soffice,
            workers,
            dpi,
            threshold,
            limit,
            enforce,
        } => {
            if work.exists() {
                std::fs::remove_dir_all(&work)?;
            }
            if out.exists() {
                std::fs::remove_dir_all(&out)?;
            }
            std::fs::create_dir_all(&out)?;
            let cfg = skriv_fidelity::Config {
                corpus,
                work,
                out: out.clone(),
                modes: Mode::ALL.to_vec(),
                visual: !no_visual,
                lo_loop: !no_lo_loop,
                soffice,
                workers,
                dpi,
                threshold,
                limit,
            };
            let run = skriv_fidelity::run(&cfg)?;
            let summary = skriv_fidelity::report::write(&run, &out)?;
            println!("{}", summary.headline());
            println!("report: {}", out.join("index.html").display());
            if enforce && !summary.kill_criteria_met() {
                std::process::exit(1);
            }
        }
    }
    Ok(())
}
