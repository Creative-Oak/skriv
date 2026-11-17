// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Reports: `report.json` (everything), `summary.md` and a self-contained
//! `index.html`. Written to be publishable as-is.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::Run;
use crate::structural::{Mode, Tally};

/// Kill criterion 1 (report §risk 1): ≥ 99% of untouched elements byte-preserved.
pub const UNTOUCHED_TARGET: f64 = 99.0;
/// Decision point 1: below ~95% structural fidelity on untouched content, lean
/// harder on verbatim slots.
pub const DECISION_POINT: f64 = 95.0;

#[derive(Clone, Debug, Default, Serialize)]
pub struct ModeSummary {
    pub untouched: Tally,
    pub untouched_by_feature: BTreeMap<String, Tally>,
    pub canonical: Tally,
    pub canonical_by_feature: BTreeMap<String, Tally>,
    pub touched: Tally,
    pub touched_by_feature: BTreeMap<String, Tally>,
    pub files_all_parts_identical: usize,
    pub files_changed: usize,
    pub visual_files: usize,
    pub visual_loss_files: Vec<String>,
    pub visual_errors: usize,
    /// Files LibreOffice could not render even in their original form.
    pub oracle_unrenderable: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Summary {
    pub files: usize,
    pub read_errors: Vec<(String, String)>,
    pub modes: BTreeMap<String, ModeSummary>,
    pub lo_loop: Tally,
    pub lo_loop_by_feature: BTreeMap<String, Tally>,
    pub lo_loop_errors: usize,
    pub slot_share_pct: f64,
    pub top_slots: Vec<(String, usize)>,
    pub median_read_ms: f64,
    pub median_write_ms: f64,
}

impl Summary {
    fn mode(&self, m: Mode) -> Option<&ModeSummary> {
        self.modes.get(m.label())
    }

    pub fn untouched_pct(&self) -> Option<f64> {
        self.mode(Mode::Touch).and_then(|m| m.untouched.pct())
    }

    pub fn kill_criteria_met(&self) -> bool {
        self.untouched_pct().is_some_and(|p| p >= UNTOUCHED_TARGET)
            && Mode::ALL.iter().all(|m| {
                self.mode(*m)
                    .is_none_or(|m| m.visual_loss_files.is_empty() && m.visual_errors == 0)
            })
    }

    /// The negative control: it should show loss. If it doesn't, the harness is broken.
    pub fn canary(&self) -> Option<String> {
        let c = self.mode(Mode::Canary)?;
        Some(format!(
            "canary (deliberate damage): {} files damaged, {} of them flagged as visible loss; canonical {}",
            c.files_changed,
            c.visual_loss_files.len(),
            c.canonical
                .pct()
                .map_or("n/a".into(), |p| format!("{p:.2}%")),
        ))
    }

    pub fn headline(&self) -> String {
        let pct = |t: Option<f64>| t.map_or("n/a".into(), |p| format!("{p:.2}%"));
        format!(
            "{} files ({} unreadable) · untouched elements byte-preserved: {} (target ≥ {UNTOUCHED_TARGET}%) · regen canonical: {} · LibreOffice loop: {} · files with visible loss: identity {}, touch {}, regen {}",
            self.files,
            self.read_errors.len(),
            pct(self.untouched_pct()),
            pct(self.mode(Mode::Regen).and_then(|m| m.canonical.pct())),
            pct(self.lo_loop.pct()),
            self.mode(Mode::Identity)
                .map_or(0, |m| m.visual_loss_files.len()),
            self.mode(Mode::Touch)
                .map_or(0, |m| m.visual_loss_files.len()),
            self.mode(Mode::Regen)
                .map_or(0, |m| m.visual_loss_files.len()),
        )
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

pub fn summarise(run: &Run) -> Summary {
    let mut s = Summary {
        files: run.files.len(),
        ..Default::default()
    };
    let mut slots: BTreeMap<String, usize> = BTreeMap::new();
    let (mut slot_bytes, mut part_bytes) = (0usize, 0usize);
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    for f in &run.files {
        if let Some(e) = &f.error
            && f.modes.is_empty()
        {
            s.read_errors.push((f.name.clone(), e.clone()));
            continue;
        }
        reads.push(f.read_ms);
        slot_bytes += f.slot_bytes;
        part_bytes += f.part_bytes;
        for (k, v) in &f.slots {
            *slots.entry(k.clone()).or_insert(0) += v;
        }
        for (mode, mr) in &f.modes {
            if *mode == Mode::Touch {
                writes.push(mr.write_ms);
            }
            let ms = s.modes.entry(mode.label().to_owned()).or_default();
            for (feat, t) in &mr.untouched {
                ms.untouched.merge(*t);
                ms.untouched_by_feature
                    .entry((*feat).to_owned())
                    .or_default()
                    .merge(*t);
            }
            for (feat, t) in &mr.canonical {
                ms.canonical.merge(*t);
                ms.canonical_by_feature
                    .entry((*feat).to_owned())
                    .or_default()
                    .merge(*t);
            }
            for (feat, t) in &mr.touched {
                ms.touched.merge(*t);
                ms.touched_by_feature
                    .entry((*feat).to_owned())
                    .or_default()
                    .merge(*t);
            }
            if mr.identical_parts.values().all(|b| *b) {
                ms.files_all_parts_identical += 1;
            } else {
                ms.files_changed += 1;
            }
            if let Some(v) = run.visual.get(&f.name).and_then(|m| m.get(mode)) {
                if v.original_unrenderable {
                    ms.oracle_unrenderable.push(f.name.clone());
                    continue;
                }
                ms.visual_files += 1;
                if v.visible_loss {
                    ms.visual_loss_files.push(f.name.clone());
                }
                if v.error.is_some() {
                    ms.visual_errors += 1;
                }
            }
        }
    }
    for l in run.lo_loop.values() {
        if l.error.is_some() {
            s.lo_loop_errors += 1;
            continue;
        }
        for (feat, t) in &l.canonical {
            s.lo_loop.merge(*t);
            s.lo_loop_by_feature
                .entry((*feat).to_owned())
                .or_default()
                .merge(*t);
        }
    }
    s.slot_share_pct = if part_bytes > 0 {
        100.0 * slot_bytes as f64 / part_bytes as f64
    } else {
        0.0
    };
    let mut top: Vec<_> = slots.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    top.truncate(25);
    s.top_slots = top;
    s.median_read_ms = median(reads);
    s.median_write_ms = median(writes);
    s
}

pub fn write(run: &Run, out: &Path) -> Result<Summary> {
    let summary = summarise(run);
    std::fs::create_dir_all(out)?;
    #[derive(Serialize)]
    struct Full<'a> {
        summary: &'a Summary,
        run: &'a Run,
    }
    std::fs::write(
        out.join("report.json"),
        serde_json::to_string_pretty(&Full {
            summary: &summary,
            run,
        })?,
    )?;
    std::fs::write(out.join("summary.md"), markdown(run, &summary))?;
    std::fs::write(out.join("index.html"), html(run, &summary))?;
    Ok(summary)
}

fn pct(t: &Tally) -> String {
    match t.pct() {
        Some(p) if t.ok == t.total => format!("{p:.0}%"),
        Some(p) => format!("{p:.2}%"),
        None => "–".into(),
    }
}

fn cell(t: Option<&Tally>) -> String {
    t.map_or("–".into(), |t| {
        format!("{} ({}/{})", pct(t), t.ok, t.total)
    })
}

fn features(s: &Summary) -> Vec<String> {
    let mut f: Vec<String> = s
        .modes
        .values()
        .flat_map(|m| m.canonical_by_feature.keys().cloned())
        .collect();
    f.sort();
    f.dedup();
    f
}

pub fn markdown(run: &Run, s: &Summary) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "# ODT round-trip fidelity\n");
    let _ = writeln!(
        o,
        "Generated {} · skriv {} · oracle {} · {} files · visual threshold {} of pixels per page at {} dpi\n",
        run.generated, run.skriv_commit, run.libreoffice, s.files, run.threshold, run.dpi
    );
    let _ = writeln!(o, "**{}**\n", s.headline());
    if let Some(c) = s.canary() {
        let _ = writeln!(
            o,
            "Negative control — {c}. If this showed no loss, the harness would be broken.\n"
        );
    }
    let _ = writeln!(o, "## Kill criteria\n");
    let _ = writeln!(
        o,
        "| Criterion | Target | Result | Met |\n|---|---|---|---|"
    );
    let u = s.untouched_pct();
    let _ = writeln!(
        o,
        "| Untouched elements byte-preserved (touch mode) | ≥ {UNTOUCHED_TARGET}% | {} | {} |",
        u.map_or("n/a".into(), |p| format!("{p:.3}%")),
        if u.is_some_and(|p| p >= UNTOUCHED_TARGET) {
            "yes"
        } else {
            "no"
        }
    );
    for m in Mode::ALL {
        if let Some(ms) = s.modes.get(m.label()) {
            let _ = writeln!(
                o,
                "| Files with visible loss, {} mode | 0 | {} of {} ({} render errors) | {} |",
                m.label(),
                ms.visual_loss_files.len(),
                ms.visual_files,
                ms.visual_errors,
                if ms.visual_loss_files.is_empty() && ms.visual_errors == 0 {
                    "yes"
                } else {
                    "no"
                }
            );
        }
    }
    let _ = writeln!(o, "\n## Per feature\n");
    let _ = writeln!(
        o,
        "Untouched = byte-identical after touching 20% of nodes. Regenerated = touched elements whose canonical form survived. Regen = canonical survival when the whole document is regenerated from the model. LO loop = LibreOffice re-saves the original and our regenerated file; canonical agreement.\n"
    );
    let _ = writeln!(
        o,
        "| Feature | Untouched (bytes) | Regenerated (touch) | Regen all (canonical) | LO loop |\n|---|---|---|---|---|"
    );
    let touch = s.modes.get("touch");
    let regen = s.modes.get("regen");
    for f in features(s) {
        let _ = writeln!(
            o,
            "| {f} | {} | {} | {} | {} |",
            cell(touch.and_then(|m| m.untouched_by_feature.get(&f))),
            cell(touch.and_then(|m| m.touched_by_feature.get(&f))),
            cell(regen.and_then(|m| m.canonical_by_feature.get(&f))),
            cell(s.lo_loop_by_feature.get(&f)),
        );
    }
    let _ = writeln!(
        o,
        "\n## What the model doesn't understand yet\n\n{:.1}% of XML bytes were carried in preservation slots. Most common slotted elements:\n",
        s.slot_share_pct
    );
    for (k, v) in &s.top_slots {
        let _ = writeln!(o, "- `{k}` × {v}");
    }
    if let Some(ms) = s.modes.get("identity")
        && !ms.oracle_unrenderable.is_empty()
    {
        let _ = writeln!(
            o,
            "\n## Files LibreOffice could not render in their original form\n\nNot scored visually.\n"
        );
        for f in &ms.oracle_unrenderable {
            let _ = writeln!(o, "- `{f}`");
        }
    }
    if !s.read_errors.is_empty() {
        let _ = writeln!(o, "\n## Files we could not read\n");
        for (f, e) in &s.read_errors {
            let _ = writeln!(o, "- `{f}`: {e}");
        }
    }
    for m in Mode::ALL {
        if let Some(ms) = s.modes.get(m.label())
            && !ms.visual_loss_files.is_empty()
        {
            // (canary files are listed in report.json, not here)
            let _ = writeln!(o, "\n## Visible differences, {} mode\n", m.label());
            for f in &ms.visual_loss_files {
                let v = &run.visual[f][&m];
                let _ = writeln!(
                    o,
                    "- `{f}`: pages {} → {}, worst page {:.3}% changed{}",
                    v.pages_original,
                    v.pages_output,
                    v.max_ratio * 100.0,
                    v.error
                        .as_ref()
                        .map(|e| format!(" ({e})"))
                        .unwrap_or_default()
                );
            }
        }
    }
    let _ = writeln!(
        o,
        "\n## Speed\n\nMedian read {:.1} ms, median write (touch mode) {:.1} ms per file.",
        s.median_read_ms, s.median_write_ms
    );
    o
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn html(run: &Run, s: &Summary) -> String {
    let mut o = String::new();
    o.push_str(r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>ODT fidelity</title><style>
:root{--bg:#fbfaf7;--fg:#1d1c1a;--muted:#6b675f;--line:#e4e0d7;--ok:#1f7a4d;--bad:#b3261e;--card:#fff}
@media (prefers-color-scheme:dark){:root{--bg:#161513;--fg:#ecebe7;--muted:#a19d94;--line:#34312c;--ok:#6fcf97;--bad:#ff8a80;--card:#1e1d1a}}
body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.5 system-ui,-apple-system,"Segoe UI",sans-serif}
main{max-width:1100px;margin:0 auto;padding:32px 16px}
h1{font-size:28px;margin:0 0 4px}h2{font-size:19px;margin:36px 0 10px}
.meta{color:var(--muted);font-size:13px}
.kpis{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:12px;margin:20px 0}
.kpi{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:14px}
.kpi b{display:block;font-size:26px;font-variant-numeric:tabular-nums}.kpi span{color:var(--muted);font-size:13px}
.ok{color:var(--ok)}.bad{color:var(--bad)}
.scroll{overflow-x:auto}
table{border-collapse:collapse;width:100%;font-variant-numeric:tabular-nums;font-size:14px}
th,td{text-align:left;padding:6px 10px;border-bottom:1px solid var(--line);vertical-align:top}th{color:var(--muted);font-weight:600}
code{font:13px ui-monospace,Menlo,monospace}
details{border-bottom:1px solid var(--line);padding:6px 0}summary{cursor:pointer}
img{max-width:100%;border:1px solid var(--line);margin:6px 0}
</style></head><body><main>"#);
    let _ = write!(
        o,
        "<h1>ODT round-trip fidelity</h1><p class=meta>Generated {} · skriv <code>{}</code> · oracle {} · {} files</p>",
        esc(&run.generated),
        esc(&run.skriv_commit),
        esc(&run.libreoffice),
        s.files
    );
    let u = s.untouched_pct();
    let met = |b: bool| if b { "ok" } else { "bad" };
    let _ = write!(
        o,
        "<div class=kpis><div class=kpi><b class={}>{}</b><span>untouched elements byte-preserved (target ≥ {UNTOUCHED_TARGET}%)</span></div>",
        met(u.is_some_and(|p| p >= UNTOUCHED_TARGET)),
        u.map_or("n/a".into(), |p| format!("{p:.3}%"))
    );
    for m in Mode::ALL {
        if let Some(ms) = s.modes.get(m.label()) {
            let _ = write!(
                o,
                "<div class=kpi><b class={}>{} / {}</b><span>files with visible loss, {} mode</span></div>",
                met(ms.visual_loss_files.is_empty() && ms.visual_errors == 0),
                ms.visual_loss_files.len(),
                ms.visual_files,
                m.label()
            );
        }
    }
    let _ = write!(
        o,
        "<div class=kpi><b>{}</b><span>LibreOffice re-save agreement</span></div><div class=kpi><b>{:.1}%</b><span>of XML carried in preservation slots</span></div></div>",
        pct(&s.lo_loop),
        s.slot_share_pct
    );
    o.push_str("<h2>Per feature</h2><div class=scroll><table><tr><th>Feature</th><th>Untouched (bytes)</th><th>Regenerated (touch)</th><th>Regen all (canonical)</th><th>LibreOffice loop</th></tr>");
    let touch = s.modes.get("touch");
    let regen = s.modes.get("regen");
    for f in features(s) {
        let _ = write!(
            o,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            esc(&f),
            cell(touch.and_then(|m| m.untouched_by_feature.get(&f))),
            cell(touch.and_then(|m| m.touched_by_feature.get(&f))),
            cell(regen.and_then(|m| m.canonical_by_feature.get(&f))),
            cell(s.lo_loop_by_feature.get(&f))
        );
    }
    o.push_str("</table></div><p class=meta>Untouched: elements outside any edited node, compared byte for byte. Regenerated: edited elements, compared canonically (attribute order and automatic-style names ignored). Regen all: every element after regenerating the whole document. LibreOffice loop: LibreOffice re-saves the original and our regenerated file, and the two are compared canonically.</p>");

    o.push_str("<h2>Files</h2>");
    for f in &run.files {
        let vis = run.visual.get(&f.name);
        let loss = vis.is_some_and(|m| m.values().any(|v| v.visible_loss));
        let touch = f.modes.get(&Mode::Touch);
        let badge = if f.error.is_some() && f.modes.is_empty() {
            "<span class=bad>unreadable</span>".to_owned()
        } else {
            let u = touch.map(|t| {
                let mut a = Tally::default();
                for v in t.untouched.values() {
                    a.merge(*v);
                }
                a
            });
            format!(
                "untouched {} · {}",
                u.map(|t| pct(&t)).unwrap_or_default(),
                if loss {
                    "<span class=bad>visible difference</span>"
                } else {
                    "<span class=ok>no visible difference</span>"
                }
            )
        };
        let _ = write!(
            o,
            "<details><summary><code>{}</code> — {badge}</summary>",
            esc(&f.name)
        );
        if let Some(e) = &f.error {
            let _ = write!(o, "<p class=bad>{}</p>", esc(e));
        }
        for (m, mr) in &f.modes {
            let _ = write!(
                o,
                "<p><b>{}</b>: parts identical {:?} · regenerated {} · passed through {} · verbatim {} · write {:.1} ms</p>",
                m.label(),
                mr.identical_parts,
                mr.regenerated_nodes,
                mr.passthrough_nodes,
                mr.verbatim_nodes,
                mr.write_ms
            );
            if let Some(v) = vis.and_then(|v| v.get(m)) {
                let _ = write!(
                    o,
                    "<p class=meta>pages {} → {}, worst page {:.3}% changed{}</p>",
                    v.pages_original,
                    v.pages_output,
                    v.max_ratio * 100.0,
                    v.error
                        .as_ref()
                        .map(|e| format!(" — {}", esc(e)))
                        .unwrap_or_default()
                );
                for img in &v.diff_images {
                    let _ = write!(
                        o,
                        "<img loading=lazy src=\"{}\" alt=\"page diff\">",
                        esc(img)
                    );
                }
            }
            if !mr.notes.is_empty() {
                o.push_str("<ul>");
                for n in &mr.notes {
                    let _ = write!(o, "<li><code>{}</code></li>", esc(n));
                }
                o.push_str("</ul>");
            }
        }
        if !f.slots.is_empty() {
            let _ = write!(
                o,
                "<p class=meta>slots: {}</p>",
                esc(&format!("{:?}", f.slots))
            );
        }
        o.push_str("</details>");
    }
    o.push_str("</main></body></html>");
    o
}
