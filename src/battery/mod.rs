//! A cross-library port of Automerge's `rust/benchmark-battery`.
//!
//! Run it with `cargo run --release --bin battery -- --help`.

pub mod compare;
pub mod experiment;
pub mod format;
pub mod generators;
pub mod graph;
pub mod memory;
pub mod registry;
pub mod report;
pub mod runner;
pub mod scenarios;
pub mod trace;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::api::Crdt;

/// Directory holding the battery's input data. Override with `CRDT_BATTERY_DATA`.
/// Populate it with `scripts/fetch-battery-data.sh`.
pub fn data_dir() -> PathBuf {
    std::env::var_os("CRDT_BATTERY_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("benches/data/battery"))
}

pub const EGWALKER_TRACES: [&str; 7] = ["S1", "S2", "S3", "A1", "A2", "C1", "C2"];

/// Sequential traces have one agent and no concurrency, so every library must
/// reproduce `endContent` exactly.
pub fn is_sequential_trace(label: &str) -> bool {
    label.starts_with('S')
}

pub fn egwalker_trace_path(label: &str) -> PathBuf {
    data_dir().join("egwalker-paper").join(format!("{label}.json"))
}

/// Where the saved document for `label` built by library `D` lives.
pub fn egwalker_fixture_path<D: Crdt>(label: &str) -> PathBuf {
    data_dir()
        .join("egwalker-paper")
        .join(format!("{label}.{}", D::NAME))
}

pub fn edits_path() -> PathBuf {
    data_dir().join("edits.json")
}

/// Replay the trace `label` with library `D`, save the result next to the
/// trace. Check the result with [`check_fixtures`].
pub fn generate_fixture<D: Crdt>(label: &str) -> Result<()> {
    let input = egwalker_trace_path(label);
    let trace = trace::Trace::load(&input)?;
    let total = trace.patch_count();
    eprintln!(
        "[{}] replaying {label}: {} transactions, {} agents, {total} patches",
        D::NAME,
        trace.txns.len(),
        trace.num_agents,
    );
    let start = std::time::Instant::now();
    let step = (total / 10).max(1);
    let mut next = step;
    let mut doc: D = trace::replay(&trace, |done| {
        if done >= next && done < total {
            eprintln!("  {done}/{total} patches");
            next += step;
        }
    });
    let output = egwalker_fixture_path::<D>(label);
    let bytes = doc.save();
    std::fs::write(&output, &bytes).with_context(|| format!("writing {}", output.display()))?;
    eprintln!(
        "  wrote {} ({} bytes) in {:.1?}",
        output.display(),
        bytes.len(),
        start.elapsed()
    );
    Ok(())
}

/// The text of library `D`'s saved fixture for `label`, if there is one.
pub fn fixture_text<D: Crdt>(label: &str) -> Result<Option<String>> {
    let path = egwalker_fixture_path::<D>(label);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let doc = D::load(&bytes);
    let text = doc
        .get_text(trace::TEXT)
        .with_context(|| format!("{} has no text object", path.display()))?;
    Ok(Some(doc.text(&text)))
}

/// How far, as a fraction of `endContent`'s length, a concurrent trace's
/// final text may be from `endContent`.
///
/// Concurrent traces are only replayable exactly when no two branches insert
/// at the same place, and some traces break that. A2 merges five independent
/// root histories, which every library orders its own way; later positions
/// then point at different text, so each library ends with a different,
/// partly garbled document (Automerge is 0.73% shorter than `endContent`, Yrs
/// 0.10%). That still gives a workload of the right size. A replay that
/// misses by more than this is treated as broken.
pub const CONCURRENT_LENGTH_TOLERANCE: f64 = 0.01;

/// Check every library's fixture for `label` against the trace.
///
/// `texts` holds `(library, final text)` for each library with a fixture.
/// Sequential traces must match `endContent` exactly (so all libraries agree).
/// Concurrent traces may differ from `endContent` and from each other, which
/// is reported, but each library's length must be within
/// [`CONCURRENT_LENGTH_TOLERANCE`] of `endContent`'s.
pub fn check_fixtures(label: &str, texts: &[(&str, String)]) -> Result<()> {
    if texts.is_empty() {
        return Ok(());
    }
    let trace = trace::Trace::load(&egwalker_trace_path(label))?;
    let expected = &trace.end_content;
    let expected_len = expected.chars().count();
    let sequential = is_sequential_trace(label);
    let mut failures = Vec::new();
    for (lib, text) in texts {
        if text == expected {
            eprintln!("{label}: {lib} matches endContent");
            continue;
        }
        let len = text.chars().count();
        let off = (len as f64 - expected_len as f64).abs() / expected_len.max(1) as f64;
        let line = format!(
            "{lib} differs from endContent: {len} vs {expected_len} chars ({:+.2}%), first difference at char {}",
            (len as f64 / expected_len.max(1) as f64 - 1.0) * 100.0,
            first_difference(text, expected),
        );
        if sequential || off > CONCURRENT_LENGTH_TOLERANCE {
            failures.push(line);
        } else {
            eprintln!("{label}: warning: {line}");
        }
    }
    if !failures.is_empty() {
        let rule = if sequential {
            "sequential traces must match exactly".to_string()
        } else {
            format!(
                "concurrent traces must be within {}% of its length",
                CONCURRENT_LENGTH_TOLERANCE * 100.0
            )
        };
        bail!("{label}: {} ({rule})", failures.join("; "));
    }
    let (first_lib, first) = &texts[0];
    match texts[1..].iter().find(|(_, text)| text != first) {
        Some((lib, _)) => eprintln!(
            "{label}: warning: {first_lib} and {lib} end with different texts (allowed for concurrent traces)"
        ),
        None if texts.len() > 1 => eprintln!("{label}: all libraries end with the same text"),
        None => {}
    }
    Ok(())
}

fn first_difference(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}
