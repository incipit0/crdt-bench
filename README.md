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

Future plans:

- Add more CRDT libraries than just Automerge and Yrs
- Add more real world workloads

