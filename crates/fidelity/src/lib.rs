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
    /// LibreOffice could not render the original either: nothing to compare.
    pub original_unrenderable: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LoLoop {
    pub canonical: BTreeMap<&'static str, Tally>,
    /// Elements that differed between two LibreOffice re-saves of the original, so not scored.
    pub unstable: usize,
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

/// A short, unique stem for a corpus file.
fn stem(i: usize) -> String {
    format!("f{i:04}")
}

pub fn run(cfg: &Config) -> Result<Run> {
    let files = collect_corpus(&cfg.corpus, cfg.limit);
    let odt_dir = cfg.work.join("odt");
    for v in std::iter::once("original").chain(cfg.modes.iter().map(|m| m.label())) {
        std::fs::create_dir_all(odt_dir.join(v))?;
    }

    eprintln!("structural: {} files", files.len());
    // Every variant of a file keeps the same file name, in its own directory:
    // documents can show their own file name (a `text:file-name` field in a
    // header), and a different name would render as a difference.
    let results: Vec<Prepared> = files
        .par_iter()
        .enumerate()
        .map(|(i, path)| {
            let bytes = std::fs::read(path).unwrap_or_default();
            let out = structural::run_file(&display_name(path), &bytes, &cfg.modes);
            let file = format!("{}.odt", stem(i));
            let orig = odt_dir.join("original").join(&file);
            std::fs::write(&orig, &bytes).ok();
            let mut written = Vec::new();
            for (mode, pkg) in out.packages {
                let p = odt_dir.join(mode.label()).join(&file);
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

/// Convert files after copying them to one staging directory, so every
/// variant of a document is opened from the same full path. Documents can show
/// their own path (`text:file-name text:display="full"`), and we must not
/// count our directory layout as a difference.
fn staged_convert(
    oracle: &render::Oracle,
    inputs: &[PathBuf],
    stage: &Path,
    format: &str,
    outdir: &Path,
) -> Result<()> {
    if stage.exists() {
        std::fs::remove_dir_all(stage)?;
    }
    std::fs::create_dir_all(stage)?;
    let mut staged = Vec::with_capacity(inputs.len());
    for p in inputs {
        let to = stage.join(p.file_name().unwrap_or_default());
        std::fs::copy(p, &to)?;
        staged.push(to);
    }
    let failed = oracle.convert(&staged, format, outdir);
    if !failed.is_empty() {
        eprintln!(
            "  {} of {} files produced no {format}",
            failed.len(),
            staged.len()
        );
    }
    Ok(())
}

fn visual_pass(
    cfg: &Config,
    oracle: &render::Oracle,
    results: &[Prepared],
    run: &mut Run,
) -> Result<()> {
    let pdf_dir = cfg.work.join("pdf");
    let png_dir = cfg.work.join("png");
    let ok: Vec<&Prepared> = results
        .iter()
        .filter(|(r, _, _)| r.error.is_none())
        .collect();
    let originals: Vec<PathBuf> = ok.iter().map(|(_, _, o)| o.clone()).collect();
    eprintln!(
        "visual: rendering {} originals and their round trips",
        originals.len()
    );
    let stage = cfg.work.join("stage");
    staged_convert(oracle, &originals, &stage, "pdf", &pdf_dir.join("original"))?;
    for mode in &cfg.modes {
        let inputs: Vec<PathBuf> = ok
            .iter()
            .filter_map(|(_, w, _)| w.iter().find(|(m, _)| m == mode).map(|(_, p)| p.clone()))
            .collect();
        staged_convert(oracle, &inputs, &stage, "pdf", &pdf_dir.join(mode.label()))?;
    }
    let diff_dir = cfg.out.join("diffs");
    std::fs::create_dir_all(&diff_dir)?;

    let per_file: Vec<(String, BTreeMap<Mode, Visual>)> = ok
        .par_iter()
        .map(|(r, written, orig)| {
            let mut m = BTreeMap::new();
            let st = orig.file_stem().unwrap().to_string_lossy().into_owned();
            let orig_pages = render::rasterise(
                &pdf_dir.join("original").join(format!("{st}.pdf")),
                &png_dir.join("original").join(&st),
                cfg.dpi,
            );
            for (mode, _) in written {
                let mut v = Visual::default();
                let out_pages = render::rasterise(
                    &pdf_dir.join(mode.label()).join(format!("{st}.pdf")),
                    &png_dir.join(mode.label()).join(&st),
                    cfg.dpi,
                );
                match (&orig_pages, out_pages) {
                    (Ok(a), Ok(b)) => {
                        v.pages_original = a.len();
                        v.pages_output = b.len();
                        for (i, (pa, pb)) in a.iter().zip(b.iter()).enumerate() {
                            let diff_name = format!("{st}-{}-p{}.png", mode.label(), i + 1);
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
                    (Err(e), _) => {
                        v.error = Some(format!("original did not render: {e}"));
                        v.original_unrenderable = true;
                    }
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
    let ok: Vec<(&Prepared, &PathBuf)> = results
        .iter()
        .filter(|(r, _, _)| r.error.is_none())
        .filter_map(|p| {
            p.1.iter()
                .find(|(m, _)| *m == Mode::Regen)
                .map(|(_, w)| (p, w))
        })
        .collect();
    let originals: Vec<PathBuf> = ok.iter().map(|(p, _)| p.2.clone()).collect();
    let ours: Vec<PathBuf> = ok.iter().map(|(_, w)| (*w).clone()).collect();
    eprintln!(
        "libreoffice loop: re-saving {} originals twice and our regenerated files once",
        originals.len()
    );
    // The original is re-saved twice: LibreOffice is not deterministic for
    // some content (an unfixed date field stores the time of saving), and
    // anything that differs between those two runs is noise, not loss.
    let stage = cfg.work.join("stage");
    staged_convert(oracle, &originals, &stage, "odt", &dir.join("a"))?;
    staged_convert(oracle, &originals, &stage, "odt", &dir.join("b"))?;
    staged_convert(oracle, &ours, &stage, "odt", &dir.join("ours"))?;
    let per_file: Vec<(String, LoLoop)> = ok
        .par_iter()
        .map(|((r, _, orig), _)| {
            let f = orig.file_name().unwrap();
            (
                r.name.clone(),
                compare_resaved(
                    &dir.join("a").join(f),
                    &dir.join("b").join(f),
                    &dir.join("ours").join(f),
                ),
            )
        })
        .collect();
    run.lo_loop = per_file.into_iter().collect();
    Ok(())
}

/// Compare LibreOffice's re-save of our file (`ours`) with its re-save of the
/// original (`a`), counting only elements that were identical across two
/// re-saves of the original (`a`, `b`).
fn compare_resaved(a: &Path, b: &Path, ours: &Path) -> LoLoop {
    let mut out = LoLoop::default();
    let read = |p: &Path, part: &str| -> Result<String> {
        let pkg = skriv_odf::package::Package::open(std::fs::read(p)?)?;
        Ok(pkg.read_string(part)?.unwrap_or_default())
    };
    type Pool<'a> = std::collections::HashMap<(&'a str, u64), usize>;
    fn pool(c: &canon::Canon) -> Pool<'_> {
        let mut p = Pool::new();
        for e in &c.elems {
            *p.entry((e.feature, e.hash)).or_insert(0) += 1;
        }
        p
    }
    fn take(p: &mut Pool<'_>, k: (&'static str, u64)) -> bool {
        match p.get_mut(&k) {
            Some(n) if *n > 0 => {
                *n -= 1;
                true
            }
            _ => false,
        }
    }
    for part in ["content.xml", "styles.xml", "meta.xml"] {
        let texts = (read(a, part), read(b, part), read(ours, part));
        let (sa, sb, so) = match texts {
            (Ok(x), Ok(y), Ok(z)) => (x, y, z),
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
                out.error = Some(format!("{part}: {e}"));
                return out;
            }
        };
        let auto = |s: &str| {
            if part == "content.xml" {
                canon::auto_styles(s)
            } else {
                Default::default()
            }
        };
        let (ma, mb, mo) = (auto(&sa), auto(&sb), auto(&so));
        let (Ok(ca), Ok(cb), Ok(co)) = (
            canon::canon(&sa, &ma),
            canon::canon(&sb, &mb),
            canon::canon(&so, &mo),
        ) else {
            out.error = Some(format!("{part}: does not parse after re-save"));
            return out;
        };
        let mut stable = pool(&cb);
        let mut theirs = pool(&co);
        for e in &ca.elems {
            if !take(&mut stable, (e.feature, e.hash)) {
                out.unstable += 1;
                continue;
            }
            let ok = take(&mut theirs, (e.feature, e.hash));
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
