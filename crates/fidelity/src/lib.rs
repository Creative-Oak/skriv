// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The ODT fidelity harness. LibreOffice is the oracle (ADR 0003).
//!
//! 1. **Structural**: round-trip every file through `skriv-odf` in three modes
//!    and compare bytes and canonical XML per element and per feature.
//! 2. **Visual**: render original and round-tripped files to PDF with
//!    LibreOffice, rasterise, and diff page images.
//! 3. **LibreOffice loop**: let LibreOffice re-save both the original and our
//!    fully regenerated output, and compare those canonically. This asks "does
//!    LibreOffice read our file the way it read the original?" while ignoring
//!    LibreOffice's own normalisation.

pub mod canon;
pub mod render;
pub mod report;
pub mod structural;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use rayon::prelude::*;
use serde::Serialize;

use structural::{Mode, Tally};

/// A file after the structural pass: its result, the packages written per
/// mode, and a copy of the original, all in the work directory.
type Prepared = (structural::FileResult, Vec<(Mode, PathBuf)>, PathBuf);

pub struct Config {
    pub corpus: Vec<PathBuf>,
    pub work: PathBuf,
    pub out: PathBuf,
    pub modes: Vec<Mode>,
    pub visual: bool,
    pub lo_loop: bool,
    pub soffice: PathBuf,
    pub workers: usize,
    pub dpi: u32,
    /// A page whose changed-pixel ratio exceeds this counts as visible loss.
    pub threshold: f64,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Visual {
    pub pages_original: usize,
    pub pages_output: usize,
    pub max_ratio: f64,
    pub pages_over_threshold: usize,
    pub visible_loss: bool,
    pub diff_images: Vec<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LoLoop {
    pub canonical: BTreeMap<&'static str, Tally>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Run {
    pub generated: String,
    pub libreoffice: String,
    pub skriv_commit: String,
    pub corpus: Vec<String>,
    pub threshold: f64,
    pub dpi: u32,
    pub files: Vec<structural::FileResult>,
    pub visual: BTreeMap<String, BTreeMap<Mode, Visual>>,
    pub lo_loop: BTreeMap<String, LoLoop>,
}

pub fn collect_corpus(dirs: &[PathBuf], limit: Option<usize>) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = dirs
        .iter()
        .flat_map(|d| {
            walkdir::WalkDir::new(d)
                .into_iter()
                .filter_map(|e| e.ok())
                .map(|e| e.into_path())
                .filter(|p| p.extension().is_some_and(|e| e == "odt" || e == "ott"))
        })
        .collect();
    files.sort();
    if let Some(n) = limit {
        files.truncate(n);
    }
    files
}

fn display_name(p: &Path) -> String {
    let parent = p
        .parent()
        .and_then(|d| d.file_name())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!(
        "{parent}/{}",
        p.file_name().unwrap_or_default().to_string_lossy()
    )
}

/// A filesystem-safe, unique stem for a corpus file.
fn stem(i: usize) -> String {
    format!("f{i:04}")
}

pub fn run(cfg: &Config) -> Result<Run> {
    let files = collect_corpus(&cfg.corpus, cfg.limit);
    let odt_dir = cfg.work.join("odt");
    std::fs::create_dir_all(&odt_dir)?;

    eprintln!("structural: {} files", files.len());
    let results: Vec<Prepared> = files
        .par_iter()
        .enumerate()
        .map(|(i, path)| {
            let bytes = std::fs::read(path).unwrap_or_default();
            let out = structural::run_file(&display_name(path), &bytes, &cfg.modes);
            let orig = odt_dir.join(format!("{}-original.odt", stem(i)));
            std::fs::write(&orig, &bytes).ok();
            let mut written = Vec::new();
            for (mode, pkg) in out.packages {
                let p = odt_dir.join(format!("{}-{}.odt", stem(i), mode.label()));
                if std::fs::write(&p, pkg).is_ok() {
                    written.push((mode, p));
                }
            }
            (out.result, written, orig)
        })
        .collect();

    let oracle = render::Oracle {
        soffice: cfg.soffice.clone(),
        profiles: cfg.work.join("profiles"),
        workers: cfg.workers,
        batch: 12,
        timeout: Duration::from_secs(30),
    };
    let mut run = Run {
        generated: now(),
        libreoffice: if cfg.visual || cfg.lo_loop {
            oracle.version()
        } else {
            "not used".into()
        },
        skriv_commit: git_commit(),
        corpus: cfg.corpus.iter().map(|p| p.display().to_string()).collect(),
        threshold: cfg.threshold,
        dpi: cfg.dpi,
        files: Vec::new(),
        visual: BTreeMap::new(),
        lo_loop: BTreeMap::new(),
    };

    if cfg.visual {
        visual_pass(cfg, &oracle, &results, &mut run)?;
    }
    if cfg.lo_loop {
        lo_loop_pass(cfg, &oracle, &results, &mut run)?;
    }
    run.files = results.into_iter().map(|(r, _, _)| r).collect();
    Ok(run)
}

fn visual_pass(
    cfg: &Config,
    oracle: &render::Oracle,
    results: &[Prepared],
    run: &mut Run,
) -> Result<()> {
    let pdf_dir = cfg.work.join("pdf");
    let mut inputs = Vec::new();
    for (r, written, orig) in results {
        if r.error.is_none() {
            inputs.push(orig.clone());
            inputs.extend(written.iter().map(|(_, p)| p.clone()));
        }
    }
    eprintln!("visual: rendering {} files with LibreOffice", inputs.len());
    oracle.convert(&inputs, "pdf", &pdf_dir);
    let png_dir = cfg.work.join("png");
    let diff_dir = cfg.out.join("diffs");
    std::fs::create_dir_all(&diff_dir)?;

    let per_file: Vec<(String, BTreeMap<Mode, Visual>)> = results
        .par_iter()
        .filter(|(r, _, _)| r.error.is_none())
        .map(|(r, written, orig)| {
            let mut m = BTreeMap::new();
            let ostem = orig.file_stem().unwrap().to_string_lossy().into_owned();
            let orig_pages = render::rasterise(
                &pdf_dir.join(format!("{ostem}.pdf")),
                &png_dir.join(&ostem),
                cfg.dpi,
            );
            for (mode, p) in written {
                let wstem = p.file_stem().unwrap().to_string_lossy().into_owned();
                let mut v = Visual::default();
                match (
                    &orig_pages,
                    render::rasterise(
                        &pdf_dir.join(format!("{wstem}.pdf")),
                        &png_dir.join(&wstem),
                        cfg.dpi,
                    ),
                ) {
                    (Ok(a), Ok(b)) => {
                        v.pages_original = a.len();
                        v.pages_output = b.len();
                        for (i, (pa, pb)) in a.iter().zip(b.iter()).enumerate() {
                            let diff_name = format!("{wstem}-p{}.png", i + 1);
                            let want_img = v.diff_images.len() < 3;
                            let diff_path = diff_dir.join(&diff_name);
                            match render::compare_pages(
                                pa,
                                pb,
                                i + 1,
                                want_img.then_some(diff_path.as_path()),
                            ) {
                                Ok(d) => {
                                    v.max_ratio = v.max_ratio.max(d.ratio);
                                    if d.ratio > cfg.threshold {
                                        v.pages_over_threshold += 1;
                                        if want_img && diff_path.exists() {
                                            v.diff_images.push(format!("diffs/{diff_name}"));
                                        }
                                    }
                                }
                                Err(e) => v.error = Some(e.to_string()),
                            }
                        }
                        v.visible_loss =
                            v.pages_original != v.pages_output || v.pages_over_threshold > 0;
                    }
                    (Err(e), _) => v.error = Some(format!("original did not render: {e}")),
                    (_, Err(e)) => {
                        v.error = Some(format!("output did not render: {e}"));
                        v.visible_loss = true;
                    }
                }
                m.insert(*mode, v);
            }
            (r.name.clone(), m)
        })
        .collect();
    run.visual = per_file.into_iter().collect();
    Ok(())
}

fn lo_loop_pass(
    cfg: &Config,
    oracle: &render::Oracle,
    results: &[Prepared],
    run: &mut Run,
) -> Result<()> {
    let dir = cfg.work.join("resaved");
    let mut inputs = Vec::new();
    for (r, written, orig) in results {
        if r.error.is_none()
            && let Some((_, p)) = written.iter().find(|(m, _)| *m == Mode::Regen)
        {
            inputs.push(orig.clone());
            inputs.push(p.clone());
        }
    }
    eprintln!("libreoffice loop: re-saving {} files", inputs.len());
    oracle.convert(&inputs, "odt", &dir);
    let per_file: Vec<(String, LoLoop)> = results
        .par_iter()
        .filter(|(r, w, _)| r.error.is_none() && w.iter().any(|(m, _)| *m == Mode::Regen))
        .map(|(r, written, orig)| {
            let regen = &written.iter().find(|(m, _)| *m == Mode::Regen).unwrap().1;
            let a = dir.join(orig.file_name().unwrap());
            let b = dir.join(regen.file_name().unwrap());
            (r.name.clone(), compare_resaved(&a, &b))
        })
        .collect();
    run.lo_loop = per_file.into_iter().collect();
    Ok(())
}

fn compare_resaved(a: &Path, b: &Path) -> LoLoop {
    let mut out = LoLoop::default();
    let read = |p: &Path, part: &str| -> Result<String> {
        let pkg = skriv_odf::package::Package::open(std::fs::read(p)?)?;
        Ok(pkg.read_string(part)?.unwrap_or_default())
    };
    for part in ["content.xml", "styles.xml", "meta.xml"] {
        let (sa, sb) = match (read(a, part), read(b, part)) {
            (Ok(x), Ok(y)) => (x, y),
            (Err(e), _) | (_, Err(e)) => {
                out.error = Some(format!("{part}: {e}"));
                return out;
            }
        };
        let (aa, ab) = if part == "content.xml" {
            (canon::auto_styles(&sa), canon::auto_styles(&sb))
        } else {
            Default::default()
        };
        let (Ok(ca), Ok(cb)) = (canon::canon(&sa, &aa), canon::canon(&sb, &ab)) else {
            out.error = Some(format!("{part}: does not parse after re-save"));
            return out;
        };
        let mut pool: std::collections::HashMap<(&str, u64), usize> = Default::default();
        for e in &cb.elems {
            *pool.entry((e.feature, e.hash)).or_insert(0) += 1;
        }
        for e in &ca.elems {
            // meta.xml after a re-save carries the save time and editing
            // statistics; those differ by construction and aren't loss.
            if part == "meta.xml"
                && matches!(
                    e.name.as_str(),
                    "office:document-meta"
                        | "office:meta"
                        | "meta:editing-duration"
                        | "dc:date"
                        | "meta:editing-cycles"
                        | "meta:generator"
                )
            {
                continue;
            }
            let ok = match pool.get_mut(&(e.feature, e.hash)) {
                Some(n) if *n > 0 => {
                    *n -= 1;
                    true
                }
                _ => false,
            };
            out.canonical.entry(e.feature).or_default().add(ok);
        }
    }
    out
}

fn now() -> String {
    std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

fn git_commit() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}
