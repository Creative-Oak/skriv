// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Concurrent table edits: 2–3 peers insert and delete rows and columns and
//! type into cells. Seeded and deterministic. The invariant: a rectangular
//! table, converged replicas, and no lost cell text. Runs per library:
//! `SKRIV_FUZZ_RUNS` (default 10,000 in release builds, 300 in debug).

mod common;

use common::*;
use skriv_crdt::CrdtDoc;
use skriv_crdt::logical::LNode;
use skriv_crdt::workload::{FuzzStats, table_fuzz_run};
use skriv_model::{Block, CellChild, TableChild};

fn runs() -> u64 {
    std::env::var("SKRIV_FUZZ_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(if cfg!(debug_assertions) { 300 } else { 10_000 })
}

/// Every row of every table in a written file spans the same number of
/// columns as the table declares.
fn odt_tables_rectangular(bytes: &[u8]) -> Result<(), String> {
    let odt = skriv_odf::read(bytes.to_vec()).map_err(|e| e.to_string())?;
    for b in &odt.doc.body {
        if let Block::Table(t) = b {
            let cols = t.columns().len();
            for r in t.rows() {
                let mut n = 0;
                for c in &r.cells {
                    if let CellChild::Cell(c) = c {
                        n += c
                            .attrs
                            .get("table:number-columns-repeated")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(1);
                    }
                }
                if n != cols {
                    return Err(format!("row with {n} cells in a {cols}-column table"));
                }
            }
            let _ = TableChild::Opaque;
        }
    }
    Ok(())
}

fn concurrent_tables<D: CrdtDoc>() {
    let bytes = corpus("tables-widths-merged.odt");
    let (l, base) = import(&bytes);
    let tid = l.tables()[0].id.clone();
    let mut doc = D::create(&l, 1);
    let mut total = FuzzStats::default();
    let n = runs();
    for seed in 0..n {
        let (merged, st) = table_fuzz_run(seed, &mut doc, &tid);
        total.add(&st);
        // Write ODT from every 10th merged state and check that nothing
        // outside the table changed and the table is rectangular.
        if seed % 10 == 0 {
            let c = write_and_check(&bytes, &merged, &base);
            c.assert_undamaged(&format!("{} seed {seed}", D::NAME));
            if let Err(e) = odt_tables_rectangular(&c.bytes) {
                total
                    .failures
                    .push(format!("seed {seed}: written ODT: {e}"));
            }
            if seed == 0 {
                keep(&format!("{}-tables-fuzz-seed0.odt", D::NAME), &c.bytes);
            }
        }
        let _ = merged
            .body
            .iter()
            .filter(|b| matches!(b, LNode::Table(_)))
            .count();
    }
    println!(
        "{}: {} runs, {} ops ({} row inserts, {} row deletes, {} column inserts, {} column deletes, {} cell edits), {} merges, {} edits into cells a concurrent row+column insert left empty, {} runs ended with no rows or no columns, {} failures",
        D::NAME,
        total.runs,
        total.ops,
        total.row_inserts,
        total.row_deletes,
        total.col_inserts,
        total.col_deletes,
        total.cell_edits,
        total.merges,
        total.lazily_created_cells,
        total.emptied_tables,
        total.failures.len()
    );
    assert!(
        total.failures.is_empty(),
        "{}: {:#?}",
        D::NAME,
        &total.failures[..total.failures.len().min(20)]
    );
}

each_backend!(concurrent_tables);
