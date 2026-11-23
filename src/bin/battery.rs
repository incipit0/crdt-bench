//! Command-line entry point for the cross-library benchmark battery.

use clap::{Parser, Subcommand, ValueEnum};
use crdt_bench::adapters::{AmDoc, YrsDoc};
use crdt_bench::api::Crdt;
use crdt_bench::battery::{
    EGWALKER_TRACES, check_fixtures, compare, data_dir, experiment, fixture_text, generate_fixture, registry,
    report, runner,
};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(about = "Cross-library CRDT benchmark battery (port of Automerge's benchmark-battery)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List registered benchmarks.
    List {
        /// Only list benchmarks whose group or name contains this string.
        #[arg(long)]
        filter: Option<String>,
        /// Benchmark tier to list.
        #[arg(long, default_value_t = registry::TierFilter::All)]
        tier: registry::TierFilter,
        /// Only list these libraries (repeatable). Default: all.
        #[arg(long = "library", value_enum)]
        libraries: Vec<Library>,
    },
    /// Run benchmarks as an experiment: results, report and metadata go in
    /// one directory under `results/` (a new one unless --append or --experiment).
    Run {
        /// Name the new experiment: `results/<timestamp>-battery-<label>/`.
        #[arg(long, conflicts_with = "experiment")]
        label: Option<String>,
        /// Use this experiment directory instead of creating one.
        #[arg(long)]
        experiment: Option<PathBuf>,
        /// Only run benchmarks whose group or name contains this string.
        #[arg(long)]
        filter: Option<String>,
        /// Benchmark tier to run. The default fast tier is suitable for frequent CI.
        #[arg(long, default_value_t = registry::TierFilter::Fast)]
        tier: registry::TierFilter,
        /// Only run these libraries (repeatable). Default: all.
        #[arg(long = "library", value_enum)]
        libraries: Vec<Library>,
        /// Number of measured samples per benchmark.
        #[arg(long, default_value_t = 30)]
        sample_count: usize,
        /// Number of warmup iterations per benchmark.
        #[arg(long, default_value_t = 3)]
        warmup_count: usize,
        /// Iterations inside each measured sample.
        #[arg(long, default_value_t = 1)]
        iters_per_sample: u64,
        /// Report output format.
        #[arg(long, default_value_t = report::Format::Markdown)]
        report_format: report::Format,
        /// Fail if the filter matches no benchmarks.
        #[arg(long, default_value_t = true)]
        fail_on_empty: bool,
        /// Add measured results to an existing experiment (the latest one
        /// unless --experiment is given), keeping its earlier results.
        #[arg(long, conflicts_with = "label")]
        append: bool,
    },
    /// Rewrite the report of an existing experiment.
    Report {
        /// Experiment directory (or its battery.json). Default: the latest experiment.
        experiment: Option<PathBuf>,
        /// Directory for the report. Default: the experiment directory.
        #[arg(long)]
        report_dir: Option<PathBuf>,
        #[arg(long, default_value_t = report::Format::Markdown)]
        report_format: report::Format,
    },
    /// Replay Eg-walker traces and save one document per library, used by the
    /// `egwalker_paper` and `memory` groups. Afterwards, checks every
    /// library's fixture (including earlier ones) against the trace's
    /// `endContent`: exactly for sequential traces, within 1% of its length
    /// for concurrent ones.
    GenerateFixtures {
        /// Trace labels (S1, S2, S3, A1, A2, C1, C2). Default: all.
        labels: Vec<String>,
        /// Only these libraries (repeatable). Default: all.
        #[arg(long = "library", value_enum)]
        libraries: Vec<Library>,
        /// Don't replay anything; only check the fixtures that already exist.
        #[arg(long)]
        check_only: bool,
    },
    /// Compare two experiments (e.g. before/after a change).
    Compare {
        /// Before experiment directory or result JSON.
        before: PathBuf,
        /// After experiment directory or result JSON.
        after: PathBuf,
        /// Output format.
        #[arg(long, default_value_t = CompareFormat::Text)]
        format: CompareFormat,
        /// Only report changed benchmarks whose median changed by at least this percentage.
        #[arg(long, default_value_t = 5.0)]
        min_change_percent: f64,
        /// Bootstrap confidence interval alpha. The default 0.05 means a 95% CI.
        #[arg(long, default_value_t = 0.05)]
        alpha: f64,
        /// Number of bootstrap resamples used by comparison.
        #[arg(long, default_value_t = 10_000)]
        bootstrap_resamples: usize,
        /// Show all benchmarks, including changes that are likely noise.
        #[arg(long)]
        show_all: bool,
        /// Directory for comparison artifacts such as series graphs.
        #[arg(long)]
        report_dir: Option<PathBuf>,
        /// Exit non-zero if any reported regression exceeds this percentage.
        #[arg(long)]
        fail_threshold: Option<f64>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, ValueEnum)]
enum Library {
    Automerge,
    Yrs,
}

impl Library {
    fn name(self) -> &'static str {
        match self {
            Self::Automerge => AmDoc::NAME,
            Self::Yrs => YrsDoc::NAME,
        }
    }
}

fn library_names(libraries: &[Library]) -> Vec<String> {
    libraries.iter().map(|l| l.name().to_string()).collect()
}

#[derive(Clone, Debug, ValueEnum)]
enum CompareFormat {
    Text,
    Markdown,
    Json,
}

impl std::fmt::Display for CompareFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text => f.write_str("text"),
            Self::Markdown => f.write_str("markdown"),
            Self::Json => f.write_str("json"),
        }
    }
}

/// An experiment directory, given either as the directory or as its results file.
fn experiment_dir(path: PathBuf) -> PathBuf {
    if path.is_file() {
        match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        }
    } else {
        path
    }
}

/// A results file, given either as the file or as its experiment directory.
fn results_path(path: PathBuf) -> PathBuf {
    if path.is_dir() {
        experiment::results_file(&path)
    } else {
        path
    }
}

fn file_url(path: &std::path::Path) -> String {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    format!("file://{}", path.display())
}

fn write_report(
    report_dir: &std::path::Path,
    run: &runner::BenchRun,
    format: report::Format,
) -> anyhow::Result<()> {
    let report = report::write_run(report_dir, run, format)?;
    if let Some(html) = report.html_path {
        eprintln!("open report: {}", file_url(&html));
    } else if let Some(markdown) = report.markdown_path {
        eprintln!("wrote markdown report: {}", markdown.display());
    }
    eprintln!(
        "add charts: .venv/bin/python scripts/bench.py battery {}",
        report_dir.display()
    );
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::List {
            filter,
            tier,
            libraries,
        } => {
            let benchmarks = registry::benchmarks();
            let selected = registry::filter(
                &benchmarks,
                filter.as_deref(),
                tier,
                &library_names(&libraries),
            );
            for benchmark in selected {
                match benchmark.unavailable_reason() {
                    Some(reason) => println!("{}  ({reason})", benchmark.name()),
                    None => println!("{}", benchmark.name()),
                }
            }
        }
        Command::Run {
            label,
            experiment,
            filter,
            tier,
            libraries,
            sample_count,
            warmup_count,
            iters_per_sample,
            report_format,
            fail_on_empty,
            append,
        } => {
            let benchmarks = registry::benchmarks();
            let selected = registry::filter(
                &benchmarks,
                filter.as_deref(),
                tier,
                &library_names(&libraries),
            );
            if selected.is_empty() && fail_on_empty {
                anyhow::bail!("no benchmarks matched filter {:?}", filter);
            }

            let dir = match (experiment, append) {
                (Some(dir), _) => {
                    std::fs::create_dir_all(&dir)?;
                    dir
                }
                (None, true) => experiment::latest()?,
                (None, false) => experiment::create(label.as_deref())?,
            };
            let output = experiment::results_file(&dir);
            eprintln!("experiment: {}", dir.display());
            experiment::record_command(&dir, &data_dir())?;

            let existing = if append {
                Some(runner::read_run(&output)?)
            } else {
                None
            };
            let options = runner::RunOptions {
                sample_count,
                warmup_count,
                iters_per_sample,
            };
            let additional = runner::run(&benchmarks, &selected, options)?;
            let run = match existing {
                Some(existing) => runner::append(existing, additional)?,
                None => additional,
            };
            runner::write_run(&output, &run)?;
            eprintln!(
                "wrote {} measured benchmark result(s) to {}",
                run.results.iter().filter(|r| r.was_run()).count(),
                output.display()
            );
            write_report(&dir, &run, report_format)?;
        }
        Command::Report {
            experiment,
            report_dir,
            report_format,
        } => {
            let dir = match experiment {
                Some(path) => experiment_dir(path),
                None => experiment::latest()?,
            };
            let run = runner::read_run(&experiment::results_file(&dir))?;
            write_report(&report_dir.unwrap_or(dir), &run, report_format)?;
        }
        Command::GenerateFixtures {
            labels,
            libraries,
            check_only,
        } => {
            let labels = if labels.is_empty() {
                EGWALKER_TRACES.iter().map(|l| l.to_string()).collect()
            } else {
                labels
            };
            let libraries = if libraries.is_empty() {
                vec![Library::Automerge, Library::Yrs]
            } else {
                libraries
            };
            for label in &labels {
                for &library in libraries.iter().filter(|_| !check_only) {
                    match library {
                        Library::Automerge => generate_fixture::<AmDoc>(label)?,
                        Library::Yrs => generate_fixture::<YrsDoc>(label)?,
                    };
                }
                // Compare against every library's fixture, including ones
                // generated by an earlier run.
                let mut texts = Vec::new();
                if let Some(text) = fixture_text::<AmDoc>(label)? {
                    texts.push((AmDoc::NAME, text));
                }
                if let Some(text) = fixture_text::<YrsDoc>(label)? {
                    texts.push((YrsDoc::NAME, text));
                }
                check_fixtures(label, &texts)?;
            }
        }
        Command::Compare {
            before,
            after,
            format,
            min_change_percent,
            alpha,
            bootstrap_resamples,
            show_all,
            report_dir,
            fail_threshold,
        } => {
            let before = runner::read_run(&results_path(before))?;
            let after = runner::read_run(&results_path(after))?;
            let options = compare::CompareOptions {
                min_change_percent,
                alpha,
                bootstrap_resamples,
                show_all,
            };
            let comparison = compare::compare(&before, &after, options)?;
            if let Some(report_dir) = &report_dir {
                let written = compare::write_series_graphs(&before, &after, report_dir)?;
                eprintln!("wrote {written} series graph(s) to {}", report_dir.display());
            }
            match format {
                CompareFormat::Text => print!("{}", compare::to_text(&comparison)),
                CompareFormat::Markdown => print!("{}", compare::to_markdown(&comparison)),
                CompareFormat::Json => println!("{}", serde_json::to_string_pretty(&comparison)?),
            }
            if let Some(threshold) = fail_threshold
                && comparison
                    .entries
                    .iter()
                    .any(|entry| entry.change_percent().is_some_and(|c| c > threshold))
            {
                anyhow::bail!("one or more benchmarks regressed by more than {threshold:.1}%");
            }
        }
    }

    Ok(())
}
