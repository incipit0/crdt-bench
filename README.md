# CRDT benchmarks

To run the benchmark do

```sh
cargo bench
```

Or if you want to run a particular benchmark then. See the `benches` directory
for what is available.

```sh
cargo bench --bench seq_add
```

## Graphs in one command

`scripts/bench.py` runs a workload and plots the results with matplotlib.
One-time setup:

```sh
python3 -m venv .venv && .venv/bin/pip install -r scripts/requirements.txt
```

Then:

```sh
# run + plot (sizes are swept without recompiling, via CRDT_* env vars)
.venv/bin/python scripts/bench.py run seq_add --sizes 6000,60000,600000
.venv/bin/python scripts/bench.py run load mem --file-sizes 6k,60k,600k
.venv/bin/python scripts/bench.py run all

# re-plot an existing run, or compare runs (e.g. Automerge 2 vs 3)
.venv/bin/python scripts/bench.py plot results/latest
.venv/bin/python scripts/bench.py run load --label am2   # with automerge = "0.6.0"
.venv/bin/python scripts/bench.py run load --label am3   # with automerge = "0.12.0"
.venv/bin/python scripts/bench.py plot results/*-am2 results/*-am3
```

Each run goes to `results/<timestamp>[-<label>]/` with the raw Criterion output,
`meta.json` (commit, crate versions, settings), and `plots/` (one PNG per
benchmark group, `index.html`, `summary.csv`, `memory.csv`). Other options:
`--sample-size`, `--filter`, `--quick`, `--formats png,svg,pdf`; see `--help`.

Benchmarks that use `BenchmarkId::new(lib, param)` with a numeric parameter
(`6000`, `60k`) are drawn as line charts over that parameter; everything else
is a bar chart per library.

Workloads:

- seq_add: This is simulating insertion of large chunk of texts, either one by
one or one (such as user typing characters in a text editor), or one large chunk 
at a time (user pasting a big text). And measure the time it takes to do local
insertion, as well as applying these updates to a second document.
- conc_add: similar to seq_add, but with more than one clients.
- load: The time it takes to load a saved document from disk.
- mem: The memory usage when loading a document from disk.
- version: measuing the time it takes to diff versions of documents and also
reverting to a previous version.

## Benchmark battery

`src/bin/battery.rs` is a port of Automerge's
[`rust/benchmark-battery`](https://github.com/automerge/automerge/tree/main/rust/benchmark-battery)
that runs every workload against each library. Scenarios are written once
against the library-agnostic `Crdt` trait in `src/api.rs` and instantiated per
library (adapters in `src/adapters/`). A benchmark's name is
`<group>/<case>/<library>`, e.g. `typing/single_char_merge/yrs`.
[`docs/battery-benchmarks.md`](docs/battery-benchmarks.md) describes what every
benchmark measures.

Libraries (`--library`, default all three):

- `automerge`: Automerge 0.12.0 from crates.io, `AutoCommit`.
- `automerge-hashless`: Automerge hashless, from Automerge's upstream
  [`hashless`](https://github.com/automerge/automerge/tree/hashless) branch,
  pulled in as the `automerge_hashless` git dependency (pinned by `rev` in
  `Cargo.toml`; the branch calls the document type `next::AutoCommit`). It
  keeps only the change hashes it needs, names history by `ChangeId`, and saves
  in the bundle format. It has no sync protocol, so the sync benchmarks are
  reported as unsupported for it.
- `yrs`: Yrs 0.28.1.

One-time setup: copy the traces from an Automerge checkout, then build each
library's documents for the Eg-walker paper traces:

```sh
scripts/fetch-battery-data.sh ../automerge/rust/benchmark-battery
cargo run --release --bin battery -- generate-fixtures
```

Data goes to `benches/data/battery/` (git-ignored; override with
`CRDT_BATTERY_DATA`). Benchmarks whose data or fixtures are missing are
reported as not runnable, with the command that fixes it.

```sh
cargo run --release --bin battery -- list                     # everything, with reasons for n/a
cargo run --release --bin battery -- run --label nightly      # new experiment: fast tier, all libraries
cargo run --release --bin battery -- run --tier all --filter typing --library yrs
cargo run --release --features memory --bin battery -- run --filter memory --tier all --append
.venv/bin/python scripts/bench.py battery                     # charts for the latest experiment, added to its report
cargo run --release --bin battery -- report                   # rewrite the latest experiment's report (tables only;
                                                              # run bench.py battery again for the charts)
cargo run --release --bin battery -- compare results/<before> results/<after>   # two experiments
```

Each `run` is an experiment with its own directory, following the
`results/<timestamp>[-<label>]/` layout of `scripts/bench.py run`
(`results/` is git-ignored):

```text
results/<YYYYmmdd-HHMMSS>-battery[-<label>]/
  battery.json   results
  meta.json      every command run against the experiment: argv, git commit,
                 rustc, automerge/yrs versions, and the size and SHA-256 of
                 each input file (traces and fixtures)
  report.md      cross-library report: tables from `battery`, charts from
                 scripts/bench.py battery
  plots/         scripts/bench.py battery output: PNG charts and index.html
results/latest-battery -> the newest experiment
```

`--append` adds results to the latest experiment (or to `--experiment DIR`)
instead of starting a new one, e.g. to add the memory benchmarks, which need
a separate build. The report has one row per workload and one column per
library, with each value's ratio to the best library on that row. It is
Markdown; pass `--report-format html` for an HTML version.

Charts are drawn only by matplotlib. `battery` writes the data and a
tables-only report that leaves an empty slot after each section
(`<!-- figures:NAME -->` … `<!-- /figures:NAME -->`). `scripts/bench.py
battery` draws the charts and fills the slots: a bar chart per group of
sampled benchmarks (median, with a min–max whisker), per-step graphs for
series benchmarks, and peak, steady-state and peak-vs-steady heap for memory.
Re-running it replaces the charts; rewriting the report with `battery report`
empties the slots until it runs again. Benchmark kinds, tiers,
the JSON format and `compare` work as in the original battery (see its README).
Differences from the original:

- Generated documents use a seeded RNG, so every library and every run gets
  the same edits.
- Results carry `library` and `base` (name without the library). A `not_run`
  result has a `reason` when the benchmark can't run at all (not ported yet,
  data missing, feature flag off). A benchmark that needs something the
  library lacks is reported with kind `unsupported` and a `reason`.
- Benchmarks that need an Automerge-only feature, or Automerge's real-world
  fixtures, are registered as not runnable for now; see
  `docs/tickets/2-automerge-only.md` and `docs/tickets/3-fixtures.md`.
- Sync messages are encoded to bytes and decoded again for every library. Yrs
  uses one round of the y-sync protocol (state vector, then diff).
- Graphs are SVG only.

`generate-fixtures` replays each Eg-walker trace (S1, S2, S3, A1, A2, C1, C2)
the way the battery's converter does, with one fixed replica ID per agent. It
refuses a trace in which one agent has concurrent transactions, since Yrs
corrupts a document when two concurrent branches share a client ID. It then
checks the final text. The sequential traces (S\*) must end with the trace's
`endContent`. A concurrent trace's text may differ from `endContent`, and from
library to library, but its length must stay within 1% of `endContent`'s;
`generate-fixtures --check-only` re-runs this check on existing fixtures.

A1, C1 and C2 replay exactly. A2 (git's `Makefile`) doesn't: it has five root
transactions (separate histories, such as gitk and git-gui, that git later
merged), and merging them means concurrent inserts at position 0, which each
library orders its own way. Later positions then point at different text, so
each library builds a different, partly garbled Makefile (Automerge 0.73%
shorter than `endContent`, Yrs 0.10%). The Automerge battery's own `A2.am` is
the same garbled document. Treat A2 results as a workload of the right size,
not as the real document.

Library-specific notes (also in the adapter docs): Yrs text indexes are
converted from code points to UTF-16 (yrs 0.28 has no code-point
`OffsetKind`); copying a Yrs document means encoding
and applying its state; Yrs has no per-change history, so `map/apply/*`
applies one update holding the whole document; `list_set` is delete plus
insert in Yrs; Yrs formatting has no "expand both" mode.

Future plans:

- Add more CRDT libraries than just Automerge and Yrs
- Add more real world workloads

