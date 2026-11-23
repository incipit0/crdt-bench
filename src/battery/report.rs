//! Cross-library report for a single run.
//!
//! Rows are workloads (`base` names); columns are libraries. Each measured
//! cell shows the value and, when more than one library was measured, how
//! many times slower/larger it is than the best library on that row.
//!
//! The report is tables only. Charts are drawn by `scripts/bench.py battery`,
//! which fills the empty figure slots this report leaves after each section
//! (see [`figure_slot`]).

use crate::battery::format::{format_bytes, format_duration};
use crate::battery::runner::{BenchResult, BenchRun, SeriesResult};
use anyhow::Context;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum Format {
    Markdown,
    Html,
    Both,
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Markdown => f.write_str("markdown"),
            Self::Html => f.write_str("html"),
            Self::Both => f.write_str("both"),
        }
    }
}

#[derive(Debug)]
pub struct Report {
    pub markdown_path: Option<PathBuf>,
    pub html_path: Option<PathBuf>,
}

pub fn write_run(report_dir: &Path, run: &BenchRun, format: Format) -> anyhow::Result<Report> {
    std::fs::create_dir_all(report_dir)
        .with_context(|| format!("creating report directory {}", report_dir.display()))?;
    let table = Table::new(run);

    let mut markdown_path = None;
    let mut html_path = None;
    if matches!(format, Format::Markdown | Format::Both) {
        let path = report_dir.join("report.md");
        std::fs::write(&path, render(&table, Style::Markdown))
            .with_context(|| format!("writing {}", path.display()))?;
        markdown_path = Some(path);
    }
    if matches!(format, Format::Html | Format::Both) {
        let path = report_dir.join("report.html");
        std::fs::write(&path, render(&table, Style::Html))
            .with_context(|| format!("writing {}", path.display()))?;
        html_path = Some(path);
    }
    Ok(Report {
        markdown_path,
        html_path,
    })
}

pub fn to_markdown(run: &BenchRun) -> String {
    render(&Table::new(run), Style::Markdown)
}

/// Results arranged by workload and library.
struct Table<'a> {
    libraries: Vec<String>,
    rows: BTreeMap<&'a str, BTreeMap<&'a str, &'a BenchResult>>,
}

type Row<'a> = BTreeMap<&'a str, &'a BenchResult>;

impl<'a> Table<'a> {
    fn new(run: &'a BenchRun) -> Self {
        let mut libraries: Vec<String> = Vec::new();
        let mut rows: BTreeMap<&str, Row<'_>> = BTreeMap::new();
        for result in &run.results {
            let (base, lib) = base_and_library(result);
            if !libraries.iter().any(|l| l == lib) {
                libraries.push(lib.to_string());
            }
            rows.entry(base).or_default().insert(lib, result);
        }
        Self { libraries, rows }
    }

    /// Rows that have at least one result matching `measured`.
    fn rows_where(&self, measured: impl Fn(&BenchResult) -> bool) -> Vec<(&'a str, &Row<'a>)> {
        self.rows
            .iter()
            .filter(|(_, libs)| libs.values().any(|r| measured(r)))
            .map(|(base, libs)| (*base, libs))
            .collect()
    }
}

/// Older or hand-written results may lack `base`/`library`; fall back to
/// splitting the name.
fn base_and_library(result: &BenchResult) -> (&str, &str) {
    if !result.library().is_empty() && !result.base().is_empty() {
        return (result.base(), result.library());
    }
    result
        .name()
        .rsplit_once('/')
        .unwrap_or((result.name(), "unknown"))
}

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Markdown,
    Html,
}

fn render(table: &Table<'_>, style: Style) -> String {
    let mut out = String::new();
    let title = "CRDT benchmark battery";
    match style {
        Style::Markdown => {
            writeln!(out, "# {title}\n").unwrap();
            writeln!(
                out,
                "Each cell is the median; `(N.N×)` is relative to the best library on that row.\n"
            )
            .unwrap();
        }
        Style::Html => {
            out.push_str("<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n");
            writeln!(out, "<title>{title}</title>").unwrap();
            out.push_str(
                "<style>body{font-family:system-ui,sans-serif;margin:2rem;line-height:1.4}\
                 table{border-collapse:collapse;margin:1rem 0 2rem}\
                 th,td{border:1px solid #ddd;padding:.35rem .55rem;text-align:right}\
                 th:first-child,td:first-child{text-align:left}\
                 th{background:#f6f6f6}td.best{font-weight:600}td.na{color:#999}\
                 h2{margin-top:2rem;border-bottom:1px solid #ddd;padding-bottom:.25rem}\
                 img{max-width:100%;height:auto;border:1px solid #ddd}\
                 figure.chart{margin:1rem 0 1.5rem;max-width:900px}\
                 figure.chart svg{width:100%;height:auto;border:1px solid #ddd}\
                 code{background:#f6f6f6;padding:.1rem .25rem;border-radius:.2rem}</style>\n",
            );
            out.push_str("</head>\n<body>\n");
            writeln!(out, "<h1>{title}</h1>").unwrap();
            out.push_str(
                "<p>Each cell is the median; (N.N×) is relative to the best library on that row. \
                 Hover over n/a cells for the reason.</p>\n",
            );
        }
    }

    if section(
        &mut out,
        style,
        table,
        "Sampled benchmarks (median time per iteration)",
        |r| match r {
            BenchResult::Sampled(r) => Some(r.median),
            _ => None,
        },
        format_duration,
    ) {
        figure_slot(&mut out, "sampled");
    }
    section(
        &mut out,
        style,
        table,
        "Series benchmarks (total time over all steps)",
        |r| match r {
            BenchResult::Series(r) => Some(r.summary.total),
            _ => None,
        },
        format_duration,
    );
    if section(
        &mut out,
        style,
        table,
        "Peak memory",
        |r| match r {
            BenchResult::Memory(r) => Some(r.peak_bytes),
            _ => None,
        },
        format_bytes,
    ) {
        figure_slot(&mut out, "memory-peak");
    }
    if section(
        &mut out,
        style,
        table,
        "Steady-state memory",
        |r| match r {
            BenchResult::Memory(r) => Some(r.steady_bytes),
            _ => None,
        },
        format_bytes,
    ) {
        figure_slot(&mut out, "memory-steady");
    }
    series_details(&mut out, style, table);
    unrunnable(&mut out, style, table);

    if style == Style::Html {
        out.push_str("</body>\n</html>\n");
    }
    out
}

/// A table of one measure, or nothing if no result has it. Returns whether
/// the section was written.
fn section(
    out: &mut String,
    style: Style,
    table: &Table<'_>,
    heading: &str,
    value: impl Fn(&BenchResult) -> Option<u64>,
    fmt: fn(u64) -> String,
) -> bool {
    let rows = table.rows_where(|r| value(r).is_some());
    if rows.is_empty() {
        return false;
    }
    let libs = &table.libraries;
    match style {
        Style::Markdown => {
            writeln!(out, "## {heading}\n").unwrap();
            writeln!(out, "| benchmark | {} |", libs.join(" | ")).unwrap();
            writeln!(out, "| --- |{}", " ---: |".repeat(libs.len())).unwrap();
        }
        Style::Html => {
            writeln!(out, "<h2>{}</h2>", escape_html(heading)).unwrap();
            out.push_str("<table><thead><tr><th>benchmark</th>");
            for lib in libs {
                write!(out, "<th>{}</th>", escape_html(lib)).unwrap();
            }
            out.push_str("</tr></thead><tbody>\n");
        }
    }
    for (base, by_lib) in rows {
        let values: Vec<Option<u64>> = libs
            .iter()
            .map(|lib| by_lib.get(lib.as_str()).and_then(|r| value(r)))
            .collect();
        let measured = values.iter().flatten().count();
        let best = values.iter().flatten().copied().min();
        let cells: Vec<Cell> = libs
            .iter()
            .zip(&values)
            .map(|(lib, v)| match (v, best) {
                (Some(v), Some(best)) => Cell::Value {
                    text: fmt(*v),
                    ratio: (measured > 1).then(|| *v as f64 / best.max(1) as f64),
                    best: measured > 1 && *v == best,
                },
                _ => Cell::Missing {
                    reason: by_lib
                        .get(lib.as_str())
                        .and_then(|r| r.unavailable_reason()),
                },
            })
            .collect();
        match style {
            Style::Markdown => {
                write!(out, "| `{base}` |").unwrap();
                for cell in cells {
                    write!(out, " {} |", cell.markdown()).unwrap();
                }
                out.push('\n');
            }
            Style::Html => {
                write!(out, "<tr><td><code>{}</code></td>", escape_html(base)).unwrap();
                for cell in cells {
                    out.push_str(&cell.html());
                }
                out.push_str("</tr>\n");
            }
        }
    }
    match style {
        Style::Markdown => out.push('\n'),
        Style::Html => out.push_str("</tbody></table>\n"),
    }
    true
}

enum Cell {
    Value {
        text: String,
        ratio: Option<f64>,
        best: bool,
    },
    Missing {
        reason: Option<String>,
    },
}

impl Cell {
    fn markdown(&self) -> String {
        match self {
            Self::Value { text, ratio, best } => {
                let ratio = ratio.map(|r| format!(" ({r:.1}×)")).unwrap_or_default();
                if *best {
                    format!("**{text}**{ratio}")
                } else {
                    format!("{text}{ratio}")
                }
            }
            Self::Missing { reason: Some(_) } => "n/a".to_string(),
            Self::Missing { reason: None } => "–".to_string(),
        }
    }

    fn html(&self) -> String {
        match self {
            Self::Value { text, ratio, best } => {
                let ratio = ratio.map(|r| format!(" ({r:.1}×)")).unwrap_or_default();
                let class = if *best { " class=\"best\"" } else { "" };
                format!("<td{class}>{text}{ratio}</td>")
            }
            Self::Missing {
                reason: Some(reason),
            } => format!("<td class=\"na\" title=\"{}\">n/a</td>", escape_html(reason)),
            Self::Missing { reason: None } => "<td class=\"na\">–</td>".to_string(),
        }
    }
}

/// An empty, named place for charts: `<!-- figures:NAME -->` and
/// `<!-- /figures:NAME -->` on their own lines. `scripts/bench.py battery`
/// replaces whatever is between the two with the charts it drew for NAME, so
/// it can be re-run; rewriting the report empties the slots again. The same
/// comments work in Markdown and HTML.
///
/// Names: `sampled`, `memory-peak`, `memory-steady`, and `series:<base>`
/// for each series benchmark.
fn figure_slot(out: &mut String, name: &str) {
    writeln!(out, "<!-- figures:{name} -->\n<!-- /figures:{name} -->\n").unwrap();
}

fn series_details(out: &mut String, style: Style, table: &Table<'_>) {
    let rows = table.rows_where(|r| matches!(r, BenchResult::Series(_)));
    if rows.is_empty() {
        return;
    }
    match style {
        Style::Markdown => out.push_str("## Series details\n\n"),
        Style::Html => out.push_str("<h2>Series details</h2>\n"),
    }
    for (base, by_lib) in rows {
        let series: Vec<(&str, &SeriesResult)> = table
            .libraries
            .iter()
            .filter_map(|lib| match by_lib.get(lib.as_str()) {
                Some(BenchResult::Series(r)) => Some((lib.as_str(), r)),
                _ => None,
            })
            .collect();
        match style {
            Style::Markdown => {
                writeln!(out, "### `{base}`\n").unwrap();
                out.push_str("| library | total | median | mean | last-window median |\n");
                out.push_str("| --- | ---: | ---: | ---: | ---: |\n");
                for (lib, r) in &series {
                    writeln!(
                        out,
                        "| {lib} | {} | {} | {} | {} |",
                        format_duration(r.summary.total),
                        format_duration(r.summary.median),
                        format_duration(r.summary.mean),
                        format_duration(r.summary.last_window_median),
                    )
                    .unwrap();
                }
                out.push('\n');
                figure_slot(out, &format!("series:{base}"));
            }
            Style::Html => {
                writeln!(out, "<h3><code>{}</code></h3>", escape_html(base)).unwrap();
                out.push_str(
                    "<table><thead><tr><th>library</th><th>total</th><th>median</th>\
                     <th>mean</th><th>last-window median</th></tr></thead><tbody>\n",
                );
                for (lib, r) in &series {
                    writeln!(
                        out,
                        "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                        escape_html(lib),
                        format_duration(r.summary.total),
                        format_duration(r.summary.median),
                        format_duration(r.summary.mean),
                        format_duration(r.summary.last_window_median),
                    )
                    .unwrap();
                }
                out.push_str("</tbody></table>\n");
                figure_slot(out, &format!("series:{base}"));
            }
        }
    }
}

fn unrunnable(out: &mut String, style: Style, table: &Table<'_>) {
    let mut entries = Vec::new();
    for (base, by_lib) in &table.rows {
        for (lib, result) in by_lib {
            if let Some(reason) = result.unavailable_reason() {
                entries.push((*base, *lib, reason));
            }
        }
    }
    if entries.is_empty() {
        return;
    }
    match style {
        Style::Markdown => {
            out.push_str(
                "## Not runnable\n\n| benchmark | library | reason |\n| --- | --- | --- |\n",
            );
            for (base, lib, reason) in entries {
                writeln!(out, "| `{base}` | {lib} | {reason} |").unwrap();
            }
            out.push('\n');
        }
        Style::Html => {
            out.push_str(
                "<h2>Not runnable</h2>\n<table><thead><tr><th>benchmark</th><th>library</th>\
                 <th>reason</th></tr></thead><tbody>\n",
            );
            for (base, lib, reason) in entries {
                writeln!(
                    out,
                    "<tr><td><code>{}</code></td><td>{}</td><td style=\"text-align:left\">{}</td></tr>",
                    escape_html(base),
                    escape_html(lib),
                    escape_html(&reason)
                )
                .unwrap();
            }
            out.push_str("</tbody></table>\n");
        }
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::to_markdown;
    use crate::battery::registry::BenchmarkKind;
    use crate::battery::runner::{
        BenchResult, BenchRun, MemoryResult, NotRunResult, SCHEMA_VERSION, SUITE_NAME,
    };

    fn memory(lib: &str, peak: u64) -> BenchResult {
        BenchResult::Memory(MemoryResult {
            name: format!("memory/test/{lib}"),
            group: "memory".to_string(),
            base: "memory/test".to_string(),
            library: lib.to_string(),
            unit: "bytes".to_string(),
            setup_duration_ns: 0,
            peak_bytes: peak,
            steady_bytes: peak / 2,
        })
    }

    #[test]
    fn libraries_are_columns_with_ratios_and_reasons() {
        let report = to_markdown(&BenchRun {
            schema: SCHEMA_VERSION,
            suite: SUITE_NAME.to_string(),
            commit: None,
            timestamp_unix_seconds: 0,
            rustc: None,
            target: None,
            profile: "release".to_string(),
            sample_count: 1,
            results: vec![
                memory("automerge", 2_048),
                memory("yrs", 4_096),
                BenchResult::NotRun(NotRunResult {
                    name: "memory/test/loro".to_string(),
                    group: "memory".to_string(),
                    base: "memory/test".to_string(),
                    library: "loro".to_string(),
                    unit: "bytes".to_string(),
                    benchmark_kind: BenchmarkKind::Memory,
                    reason: Some("unsupported: loro has no x".to_string()),
                }),
            ],
        });

        assert!(
            report.contains("| benchmark | automerge | yrs | loro |"),
            "{report}"
        );
        assert!(
            report.contains("| `memory/test` | **2.00 KiB** (1.0×) | 4.00 KiB (2.0×) | n/a |"),
            "{report}"
        );
        assert!(report.contains("## Steady-state memory"));
        assert!(report.contains("unsupported: loro has no x"));
        // An empty figure slot after each memory table, none for sections
        // that have no rows.
        let peak_table = report.find("## Peak memory").unwrap();
        let peak_slot = report
            .find("<!-- figures:memory-peak -->\n<!-- /figures:memory-peak -->")
            .unwrap();
        let steady_table = report.find("## Steady-state memory").unwrap();
        assert!(peak_table < peak_slot && peak_slot < steady_table);
        assert!(report.contains("<!-- figures:memory-steady -->"));
        assert!(!report.contains("figures:sampled"), "{report}");
    }
}
