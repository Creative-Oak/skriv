// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! LibreOffice as the oracle: headless conversion and page-image comparison.
//!
//! soffice is run in batches, each worker with its own user profile (two
//! instances sharing a profile block each other). Rasterising uses poppler's
//! `pdftoppm`, a test-time tool only.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use rayon::prelude::*;

pub struct Oracle {
    pub soffice: PathBuf,
    pub profiles: PathBuf,
    pub workers: usize,
    pub batch: usize,
    pub timeout: Duration,
}

impl Oracle {
    pub fn version(&self) -> String {
        Command::new(&self.soffice)
            .arg("--version")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_else(|| "unknown".into())
    }

    /// Convert `inputs` to `format` (e.g. `pdf`, `odt`) into `outdir`.
    /// Returns the inputs that produced no output.
    pub fn convert(&self, inputs: &[PathBuf], format: &str, outdir: &Path) -> Vec<PathBuf> {
        std::fs::create_dir_all(outdir).ok();
        let ext = format.split(':').next().unwrap_or(format);
        let chunks: Vec<Vec<PathBuf>> = inputs
            .chunks(self.batch.max(1))
            .map(<[_]>::to_vec)
            .collect();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.workers.max(1))
            .build()
            .expect("thread pool");
        pool.install(|| {
            chunks.par_iter().for_each(|chunk| {
                // One profile per pool thread: two soffice processes sharing a
                // profile block each other.
                let t = rayon::current_thread_index().unwrap_or(0);
                let profile = self.profiles.join(format!("p{t}"));
                let _ = self.run_batch(chunk, format, outdir, &profile);
            })
        });
        inputs
            .iter()
            .filter(|p| {
                let stem = p.file_stem().unwrap_or_default();
                !outdir.join(stem).with_extension(ext).exists()
            })
            .cloned()
            .collect()
    }

    fn run_batch(
        &self,
        files: &[PathBuf],
        format: &str,
        outdir: &Path,
        profile: &Path,
    ) -> Result<()> {
        std::fs::create_dir_all(profile)?;
        (|| {
            let mut child = Command::new(&self.soffice)
                .arg(format!(
                    "-env:UserInstallation=file://{}",
                    profile.display()
                ))
                .args([
                    "--headless",
                    "--norestore",
                    "--nolockcheck",
                    "--convert-to",
                    format,
                ])
                .arg("--outdir")
                .arg(outdir)
                .args(files)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .context("starting soffice")?;
            let limit = self.timeout * files.len() as u32;
            let t = Instant::now();
            loop {
                if child.try_wait()?.is_some() {
                    return Ok(());
                }
                if t.elapsed() > limit {
                    let _ = child.kill();
                    let _ = child.wait();
                    bail!("soffice timed out");
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        })()
    }
}

/// Render a PDF's pages to PNG files in `dir`, returning them in page order.
pub fn rasterise(pdf: &Path, dir: &Path, dpi: u32) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let status = Command::new("pdftoppm")
        .args(["-r", &dpi.to_string(), "-png"])
        .arg(pdf)
        .arg(dir.join("page"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("running pdftoppm (brew install poppler / apt install poppler-utils)")?;
    if !status.success() {
        bail!("pdftoppm failed on {}", pdf.display());
    }
    let mut pages: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|e| e == "png") && !p.to_string_lossy().contains("diff")
        })
        .collect();
    // pdftoppm zero-pads to the width of the page count, so a plain sort is page order.
    pages.sort();
    Ok(pages)
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct PageDiff {
    pub page: usize,
    /// Fraction of pixels whose grey level differs by more than the tolerance.
    pub ratio: f64,
    pub size_mismatch: bool,
}

/// Compare two page images. Writes a diff image (changed pixels red over a
/// faded original) when `diff_out` is given and anything differs.
pub fn compare_pages(a: &Path, b: &Path, page: usize, diff_out: Option<&Path>) -> Result<PageDiff> {
    let ia = image::open(a)?.to_luma8();
    let ib = image::open(b)?.to_luma8();
    if ia.dimensions() != ib.dimensions() {
        return Ok(PageDiff {
            page,
            ratio: 1.0,
            size_mismatch: true,
        });
    }
    let (w, h) = ia.dimensions();
    let mut changed = 0u64;
    let mut out = diff_out.map(|_| image::RgbImage::new(w, h));
    for (x, y, pa) in ia.enumerate_pixels() {
        let pb = ib.get_pixel(x, y);
        let d = (pa.0[0] as i16 - pb.0[0] as i16).unsigned_abs();
        let hit = d > 48;
        if hit {
            changed += 1;
        }
        if let Some(img) = out.as_mut() {
            let g = 200 + (pa.0[0] as u16 * 55 / 255) as u8;
            img.put_pixel(
                x,
                y,
                if hit {
                    image::Rgb([220, 0, 0])
                } else {
                    image::Rgb([g, g, g])
                },
            );
        }
    }
    let ratio = changed as f64 / (w as f64 * h as f64);
    if changed > 0
        && let (Some(img), Some(p)) = (out, diff_out)
    {
        img.save(p)?;
    }
    Ok(PageDiff {
        page,
        ratio,
        size_mismatch: false,
    })
}
