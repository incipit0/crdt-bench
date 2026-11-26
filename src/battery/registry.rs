//! Benchmark definitions and the registry of every benchmark the binary knows.
//!
//! A benchmark's full name is `<group>/<case>/<library>`, e.g.
//! `build/build_typing/yrs`. `base` is the name without the library segment, so
//! results for the same workload can be lined up across libraries.

use crate::api::{Crdt, Feature};

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkKind {
    Sampled,
    Series,
    Memory,
}

impl std::fmt::Display for BenchmarkKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sampled => f.write_str("sampled"),
            Self::Series => f.write_str("series"),
            Self::Memory => f.write_str("memory"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tier {
    Fast,
    Slow,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum TierFilter {
    Fast,
    Slow,
    All,
}

impl std::fmt::Display for TierFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fast => f.write_str("fast"),
            Self::Slow => f.write_str("slow"),
            Self::All => f.write_str("all"),
        }
    }
}

/// Identity shared by every kind of benchmark.
#[derive(Clone, Debug)]
pub struct Id {
    pub group: &'static str,
    /// `<group>/<case>`, without the library.
    pub base: String,
    pub library: &'static str,
    /// `<base>/<library>`.
    pub name: String,
}

impl Id {
    pub fn new(group: &'static str, base: impl Into<String>, library: &'static str) -> Self {
        let base = base.into();
        debug_assert!(
            base.starts_with(group),
            "benchmark {base} must start with its group {group}"
        );
        let name = format!("{base}/{library}");
        Self {
            group,
            base,
            library,
            name,
        }
    }
}

pub struct Runner {
    run_sample: Box<dyn FnMut(u64) -> u64>,
}

impl Runner {
    fn batched<State, Run, Output>(state: State, mut run: Run) -> Self
    where
        State: Clone + 'static,
        Run: FnMut(State) -> Output + 'static,
        Output: 'static,
    {
        Self {
            run_sample: Box::new(move |iterations| {
                let mut inputs = (0..iterations).map(|_| state.clone()).collect::<Vec<_>>();
                let mut outputs = Vec::with_capacity(inputs.len());
                let start = std::time::Instant::now();
                for input in inputs.drain(..) {
                    outputs.push(run(input));
                }
                let elapsed = elapsed_per_iteration(start, iterations);
                std::hint::black_box(&outputs);
                drop(outputs);
                drop(inputs);
                elapsed
            }),
        }
    }

    /// Like `batched`, but the input is rebuilt by `make` for each iteration
    /// (outside the timer) instead of cloned. For libraries whose documents
    /// can't be cloned cheaply or at all.
    fn rebuilt<State, Make, Run, Output>(mut make: Make, mut run: Run) -> Self
    where
        Make: FnMut() -> State + 'static,
        Run: FnMut(State) -> Output + 'static,
        State: 'static,
        Output: 'static,
    {
        Self {
            run_sample: Box::new(move |iterations| {
                let mut inputs = (0..iterations).map(|_| make()).collect::<Vec<_>>();
                let mut outputs = Vec::with_capacity(inputs.len());
                let start = std::time::Instant::now();
                for input in inputs.drain(..) {
                    outputs.push(run(input));
                }
                let elapsed = elapsed_per_iteration(start, iterations);
                std::hint::black_box(&outputs);
                drop(outputs);
                elapsed
            }),
        }
    }

    pub fn run_sample(&mut self, iterations: u64) -> u64 {
        (self.run_sample)(iterations)
    }
}

pub struct SampledBenchmark {
    pub id: Id,
    pub make_runner: Box<dyn Fn() -> Runner>,
}

pub struct SeriesBenchmark {
    pub id: Id,
    pub steps: usize,
    pub setup: Box<dyn Fn() -> Box<dyn FnMut(usize)>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryMeasurement {
    pub peak_bytes: u64,
    pub steady_bytes: u64,
}

type MemorySetup = Box<dyn Fn() -> Box<dyn FnMut() -> MemoryMeasurement>>;

pub struct MemoryBenchmark {
    pub id: Id,
    /// `None` when the binary was built without the `memory` feature.
    pub setup: Option<MemorySetup>,
}

/// A benchmark that is registered but can never run for this library, either
/// because the library lacks a feature or because it hasn't been ported yet.
pub struct Unrunnable {
    pub id: Id,
    pub kind: BenchmarkKind,
    pub reason: String,
    /// The library lacks something the benchmark needs, as opposed to the
    /// benchmark not being ported yet or missing data. Reported as
    /// `unsupported` instead of `not_run`.
    pub unsupported: bool,
}

pub enum Benchmark {
    Sampled(SampledBenchmark),
    Series(SeriesBenchmark),
    Memory(MemoryBenchmark),
    Unrunnable(Unrunnable),
}

impl Benchmark {
    pub fn id(&self) -> &Id {
        match self {
            Self::Sampled(b) => &b.id,
            Self::Series(b) => &b.id,
            Self::Memory(b) => &b.id,
            Self::Unrunnable(b) => &b.id,
        }
    }

    pub fn group(&self) -> &'static str {
        self.id().group
    }

    pub fn name(&self) -> &str {
        &self.id().name
    }

    pub fn tier(&self) -> Tier {
        if is_fast(&self.id().base) {
            Tier::Fast
        } else {
            Tier::Slow
        }
    }

    pub fn kind(&self) -> BenchmarkKind {
        match self {
            Self::Sampled(_) => BenchmarkKind::Sampled,
            Self::Series(_) => BenchmarkKind::Series,
            Self::Memory(_) => BenchmarkKind::Memory,
            Self::Unrunnable(b) => b.kind,
        }
    }

    pub fn unit(&self) -> &'static str {
        unit(self.kind())
    }

    /// Why this benchmark can't run in this build, if it can't.
    pub fn unavailable_reason(&self) -> Option<&str> {
        match self {
            Self::Sampled(_) | Self::Series(_) => None,
            Self::Memory(b) => b
                .setup
                .is_none()
                .then_some("built without the `memory` feature"),
            Self::Unrunnable(b) => Some(&b.reason),
        }
    }

    /// True when the library can't do what this benchmark needs.
    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unrunnable(b) if b.unsupported)
    }
}

pub fn unit(kind: BenchmarkKind) -> &'static str {
    match kind {
        BenchmarkKind::Sampled => "ns/iter",
        BenchmarkKind::Series => "ns/step",
        BenchmarkKind::Memory => "bytes",
    }
}

fn elapsed_per_iteration(start: std::time::Instant, iterations: u64) -> u64 {
    nanos_u64(start.elapsed().as_nanos() / u128::from(iterations))
}

fn nanos_u64(nanos: u128) -> u64 {
    nanos.try_into().unwrap_or(u64::MAX)
}

// ----- constructors used by scenarios -----

/// Setup runs once; the returned closure is timed on every iteration.
pub fn sampled<D: Crdt>(
    group: &'static str,
    base: impl Into<String>,
    setup: impl Fn() -> Box<dyn FnMut()> + 'static,
) -> Benchmark {
    Benchmark::Sampled(SampledBenchmark {
        id: Id::new(group, base, D::NAME),
        make_runner: Box::new(move || {
            let mut operation = setup();
            Runner::batched((), move |()| operation())
        }),
    })
}

/// `setup` runs once; each iteration gets a clone of its output (untimed) and
/// `run` is timed. Outputs are dropped outside the timer.
pub fn batched<D, Setup, Run, State, Output>(
    group: &'static str,
    base: impl Into<String>,
    setup: Setup,
    run: Run,
) -> Benchmark
where
    D: Crdt,
    Setup: Fn() -> State + 'static,
    Run: FnMut(State) -> Output + Clone + 'static,
    State: Clone + 'static,
    Output: 'static,
{
    Benchmark::Sampled(SampledBenchmark {
        id: Id::new(group, base, D::NAME),
        make_runner: Box::new(move || Runner::batched(setup(), run.clone())),
    })
}

/// Like [`batched`], but each iteration's input comes from calling `make`
/// (untimed). `prepare` runs once and builds `make`, so expensive shared
/// inputs (saved bytes, parsed traces) are computed once.
pub fn rebuilt<D, Prepare, Make, Run, State, Output>(
    group: &'static str,
    base: impl Into<String>,
    prepare: Prepare,
    run: Run,
) -> Benchmark
where
    D: Crdt,
    Prepare: Fn() -> Make + 'static,
    Make: FnMut() -> State + 'static,
    Run: FnMut(State) -> Output + Clone + 'static,
    State: 'static,
    Output: 'static,
{
    Benchmark::Sampled(SampledBenchmark {
        id: Id::new(group, base, D::NAME),
        make_runner: Box::new(move || Runner::rebuilt(prepare(), run.clone())),
    })
}

pub fn series<D: Crdt>(
    group: &'static str,
    base: impl Into<String>,
    steps: usize,
    setup: impl Fn() -> Box<dyn FnMut(usize)> + 'static,
) -> Benchmark {
    Benchmark::Series(SeriesBenchmark {
        id: Id::new(group, base, D::NAME),
        steps,
        setup: Box::new(setup),
    })
}

#[cfg(feature = "memory")]
pub fn memory<D: Crdt>(
    group: &'static str,
    base: impl Into<String>,
    setup: impl Fn() -> Box<dyn FnMut() -> MemoryMeasurement> + 'static,
) -> Benchmark {
    Benchmark::Memory(MemoryBenchmark {
        id: Id::new(group, base, D::NAME),
        setup: Some(Box::new(setup)),
    })
}

#[cfg(not(feature = "memory"))]
pub fn memory<D: Crdt>(
    group: &'static str,
    base: impl Into<String>,
    _setup: impl Fn() -> Box<dyn FnMut() -> MemoryMeasurement> + 'static,
) -> Benchmark {
    Benchmark::Memory(MemoryBenchmark {
        id: Id::new(group, base, D::NAME),
        setup: None,
    })
}

/// Build the benchmark only if `D` has every feature in `features`; otherwise
/// register it as unsupported with the reason.
pub fn requires<D: Crdt>(
    features: &[Feature],
    kind: BenchmarkKind,
    group: &'static str,
    base: impl Into<String>,
    build: impl FnOnce(String) -> Benchmark,
) -> Benchmark {
    let base = base.into();
    let missing: Vec<_> = features
        .iter()
        .filter(|f| !D::supports(**f))
        .map(|f| f.describe())
        .collect();
    if missing.is_empty() {
        build(base)
    } else {
        Benchmark::Unrunnable(Unrunnable {
            id: Id::new(group, base, D::NAME),
            kind,
            reason: format!("{} has no {}", D::NAME, missing.join(", ")),
            unsupported: true,
        })
    }
}

/// [`requires`] for a benchmark already built: keep it if `D` has every
/// feature in `features`, otherwise report it as unsupported under the same id.
/// Building is cheap (setup runs only when the benchmark does), so scenarios
/// can build first and gate after.
pub fn gate<D: Crdt>(features: &[Feature], benchmark: Benchmark) -> Benchmark {
    let (kind, group, base) = (
        benchmark.kind(),
        benchmark.group(),
        benchmark.id().base.clone(),
    );
    requires::<D>(features, kind, group, base, |_| benchmark)
}

pub fn unrunnable<D: Crdt>(
    kind: BenchmarkKind,
    group: &'static str,
    base: impl Into<String>,
    reason: impl Into<String>,
) -> Benchmark {
    Benchmark::Unrunnable(Unrunnable {
        id: Id::new(group, base, D::NAME),
        kind,
        reason: reason.into(),
        unsupported: false,
    })
}

// ----- registry -----

pub fn benchmarks() -> Vec<Benchmark> {
    let mut benchmarks = Vec::new();
    super::scenarios::register::<crate::adapters::AmDoc>(&mut benchmarks);
    super::scenarios::register::<crate::adapters::AmHashlessDoc>(&mut benchmarks);
    super::scenarios::register::<crate::adapters::YrsDoc>(&mut benchmarks);
    benchmarks
}

pub fn filter<'a>(
    benchmarks: &'a [Benchmark],
    filter: Option<&str>,
    tier_filter: TierFilter,
    libraries: &[String],
) -> Vec<&'a Benchmark> {
    benchmarks
        .iter()
        .filter(|b| tier_matches(b.tier(), tier_filter))
        .filter(|b| libraries.is_empty() || libraries.iter().any(|l| l == b.id().library))
        .filter(|b| filter.is_none_or(|f| b.group().contains(f) || b.name().contains(f)))
        .collect()
}

fn tier_matches(tier: Tier, filter: TierFilter) -> bool {
    match filter {
        TierFilter::Fast => tier == Tier::Fast,
        TierFilter::Slow => tier == Tier::Slow,
        TierFilter::All => true,
    }
}

/// Fast-tier membership by base name, so a workload is in the same tier for
/// every library. Mirrors the Automerge battery's list.
fn is_fast(base: &str) -> bool {
    matches!(
        base,
        "build/build_big_paste"
            | "build/build_text_splice_100"
            | "egwalker_paper/get_text/A1"
            | "length/text_len_now"
            | "length/text_len_at"
            | "length/list_len_now"
            | "length/map_len_now"
            | "length/map_len_at"
            | "list/list_cursor_now"
            | "list/list_cursor_at"
            | "list/list_update_now"
            | "list/list_update_at"
            | "list/list_splice_index_now"
            | "list/list_splice_index_at"
            | "load_save/load_big_paste"
            | "load_save/save_big_paste"
            | "load_save/load_text_splice_100"
            | "load_save/save_text_splice_100"
            | "load_save/save_typing"
            | "map/increasing_put/1000"
            | "map/decreasing_put/1000"
            | "map/repeated_put/1000"
            | "map/repeated_increment/1000"
            | "map/deep_history/1000"
            | "marks/add_mark"
            | "marks/splice_without_marks"
            | "range/range/10000"
            | "range/range_at/10000"
            | "sync/full_many_tx"
            | "sync/full_one_tx"
            | "sync/full_one_tx/100"
            | "sync/full_one_tx/1000"
            | "sync/every_change/100"
            | "sync/big_chunky_sync_message"
            | "typing/single_char_100_bulk_incremental_load"
    ) || base.contains("godot-modeled")
}
