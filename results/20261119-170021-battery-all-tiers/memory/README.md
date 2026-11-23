# Memory: why Automerge and Yrs use the heap they do

This experiment's `memory/load/egwalker_paper/*` results show two things to
explain:

- **Peak** heap while loading is far higher for Automerge (58–680 MiB) than its
  **steady** heap once loaded (1.4–30 MiB).
- **Steady** heap is lower for Automerge on five traces, despite Yrs's merged
  runs, but higher on C1 and C2 (18 and 30 MiB against Yrs's 10 and 7 MiB).

Investigated on 2026-11-23 with automerge 0.12.0, yrs 0.28.1 and rustc 1.96.1,
using the fixtures in `benches/data/battery/egwalker-paper/`. "Steady" means
live heap bytes once `load` has returned, measured with a counting global
allocator. The probe reproduces the report's `memory/load` numbers exactly.

## Summary

- **Automerge's ops are tiny in memory.** Its hexane columns take 1.5–2.7 MiB
  for every trace, whether it has 0.5M or 1.2M ops.
- **Its steady memory is driven by the change graph**, about 160–210 B per
  change, uncompressed. C1 and C2 have 93k and 134k changes (about 4.5 ops
  each), so the graph is most of their memory. Yrs keeps nothing per
  transaction.
- **The two hash fields make up most of the graph**: `nodes_by_hash` and
  `hashes`, with each hash stored twice. In C2 they account for 65% of the
  graph and 57% of the whole document.
- **Several graph Vecs are about half empty**, because they grow by doubling.
  Sizing them exactly cuts C2 from 30.3 to 24.5 MiB.
- **The load peak is temporary.** Load rebuilds every change to compute its
  hash, and while it does, each op becomes a ~250 B heap struct in a
  `BTreeMap` re-sort queue, including about 1.1M recreated delete ops for
  S3. All of it is freed before `load` returns.

## Battery numbers, reproduced

| Trace | Changes | Actors | AM steady | AM peak | Yrs steady | Yrs peak |
|---|---:|---:|---:|---:|---:|---:|
| S1 | 2 | 2 | 1.38 MiB | 234.7 MiB | 6.80 MiB | 7.7 MiB |
| S2 | 2 | 2 | 2.01 MiB | 317.5 MiB | 8.54 MiB | 9.8 MiB |
| S3 | 2 | 2 | 2.73 MiB | 680.1 MiB | 8.97 MiB | 10.1 MiB |
| A1 | 956 | 205 | 3.02 MiB | 91.3 MiB | 9.58 MiB | 10.7 MiB |
| A2 | 3,208 | 376 | 5.49 MiB | 57.7 MiB | 7.43 MiB | 8.4 MiB |
| C1 | 93,152 | 3 | 18.25 MiB | 180.9 MiB | 9.97 MiB | 11.4 MiB |
| C2 | 134,477 | 4 | 30.31 MiB | 208.1 MiB | 6.87 MiB | 7.8 MiB |

S1, S2 and S3 each have 2 changes because each source trace is a single
transaction (`txns: 1` in the JSON). The second change is the one that
creates the text object.

## Steady memory

### Automerge: the change graph, not the ops

To separate the cost of ops from the cost of changes, the probe built
synthetic Automerge documents with the same 600,000 typed characters, varying
only how many ops each change holds and how many actors take turns:

| Ops per change | Actors | Changes | File size | Steady memory | Extra per change |
|---:|---:|---:|---:|---:|---:|
| 600,000 | 1 | 1 | 787 B | 0.58 MiB | — |
| 4 | 1 | 150k | 805 B | 27.3 MiB | ~187 B |
| 4 | 2 | 150k | 37 KB | 31.5 MiB | ~216 B |
| 4 | 4 | 150k | 34 KB | 32.8 MiB | ~225 B |

The ops are identical in every row, so the jump from 0.58 to 33 MiB is all
per-change cost. Alternating actors adds about 30 B per change on top, from
shorter runs in the op columns plus more clock bookkeeping.

The same holds with one op per change. Here 200,000 typed characters are
committed every N ops (one actor):

| Ops per change | Changes | Saved file | Steady memory | Extra per change |
|---:|---:|---:|---:|---:|
| 200,000 | 2 | 399 B | 0.21 MB | — |
| 1,000 | 201 | 410 B | 0.24 MB | ~142 B |
| 100 | 2,001 | 410 B | 0.53 MB | ~158 B |
| 10 | 20,001 | 417 B | 3.8 MB | ~177 B |
| 1 | 200,001 | 415 B | 29.3 MB | ~146 B |

On disk the per-change metadata (actor, seq, dependencies) compresses to
almost nothing: 200k changes add 16 bytes to the file. In memory most of it
is stored as plain `Vec`s and `HashMap`s, and the hashes are random 32-byte
values that couldn't be compressed anyway.

### Where the change graph's bytes go

These numbers come from a patched copy of automerge 0.12.0 that reports each
`ChangeGraph` field's heap (see *Reproducing*). Vecs are counted as
`capacity × size_of`. HashMaps, the BTreeSet and hexane columns were cloned
under the counting allocator; hashbrown's clone keeps the same bucket count.
The parts add up to the measured total.

**C2: 30.31 MiB resident, 134,477 changes, 4 actors**

| Part of the document | MiB |
|---|---:|
| Ops (`OpSet`, compressed columns) | 1.50 |
| **Change graph** | **26.5** |
| `authors` | 2.05 |

| `ChangeGraph` field | MiB | Share of graph | Bytes per change |
|---|---:|---:|---:|
| `nodes_by_hash: HashMap<ChangeHash, NodeIdx>` | 9.25 | 35% | 72 |
| `hashes: Vec<ChangeHash>` | 8.00 | 30% | 62 |
| `clock_cache: HashMap<NodeIdx, SeqClock>` (35,947 entries) | 2.61 | 10% | 20 |
| `edges: Vec<Edge>` (230,849 dependency edges) | 2.00 | 7.5% | 16 |
| `actors`, `seq`, `max_ops` (`Vec<u32>`-sized, 1.00 each) | 3.00 | 11% | 23 |
| `seq_index`, `parents`, `num_ops` (hexane), the rest | 1.6 | 6% | 12 |
| **Total** | **26.5** | | **207** |

- **Hashes are stored twice:** 134,477 × 32 B = 4.1 MiB of real data, once in
  the `hashes` Vec and again as keys of `nodes_by_hash`.
- **`hashes` holds 8.00 MiB for 4.1 MiB of data.** It is filled by `push` in a
  loop and doubled up to 262,144 slots. The next section covers this.
- **`nodes_by_hash` is half empty by construction.** 134k entries need at
  least 153k buckets at hashbrown's 7/8 load factor, which rounds up to
  262,144 buckets × (36 B entry + 1 control byte) = 9.25 MiB. Reserving
  capacity up front doesn't change this.

**C1: 18.25 MiB, 93,152 changes, 3 actors.** Same pattern: ops 1.73 MiB, graph
14.8 MiB (167 B per change), of which `nodes_by_hash` is 4.63 MiB, `hashes`
4.00 MiB, `edges` 2.00 MiB and `clock_cache` 1.31 MiB.

**A2 is a different case: 376 actors.** A2 has only 3,208 changes, but its
graph is 3.6 MiB, and **86% of that is `clock_cache`** (3.11 MiB, about 1 KB
per change). Each cached `SeqClock` is a `Vec<Option<NonZeroU32>>` with one
slot per actor (376 × 4 B ≈ 1.5 KB), and there are 2,080 of them. That cost
grows with changes × actors.

| A2 field | MiB | Share of graph |
|---|---:|---:|
| `clock_cache` | 3.11 | 86% |
| `nodes_by_hash` | 0.14 | 4% |
| `hashes` | 0.12 | 3.5% |
| `edges` | 0.12 | 3.5% |
| Everything else | 0.12 | 3% |

`authors` (2.05 MiB in C2, about 16 B per change) isn't part of `ChangeGraph`
but also grows with change count. I didn't look into why.

### Applied to the Eg-walker traces

| Trace | Changes | AM steady | Of which change graph | Yrs steady |
|---|---:|---:|---:|---:|
| S3 | 2 | 2.73 MiB | ~0 | 8.97 MiB |
| A2 | 3,208 | 5.49 MiB | 3.6 MiB (mostly `clock_cache`) | 7.43 MiB |
| C1 | 93,152 | 18.25 MiB | 14.8 MiB | 9.97 MiB |
| C2 | 134,477 | 30.31 MiB | 26.5 MiB | 6.87 MiB |

### Why Yrs stays at 7–10 MiB on every trace

Yrs keeps nothing per transaction. After commit, a transaction leaves only the
state vector: one clock per client, so 4 entries for C2. Yrs's memory scales
with the number of items. Runs of consecutive same-client inserts are merged,
and deleted content is collected down to tombstone lengths. Each item is a
full struct of roughly 100+ B, which is why Yrs loses on S1–S3 and A1–A2.

So the crossover is roughly:

- **Automerge:** about a byte or two per op, plus about 200 B per change (more
  when there are many actors, through `clock_cache`).
- **Yrs:** about 100+ B per merged item, and nothing per transaction.

Automerge wins when changes are large (the S and A traces) and loses when
there are many small changes (C1 and C2, about 4.5 ops each).

### Right-sizing the graph's Vecs

Several `ChangeGraph` Vecs are filled during load without knowing their final
length, so they grow by doubling and keep up to half their capacity empty.

| Vec (`change_graph.rs`) | How it's filled | Fix tested |
|---|---|---|
| `hashes: Vec<ChangeHash>` | `push` per change in `finalize` (line 909), although `changes.len()` is known | `reserve_exact(changes.len())` before the loop |
| `actors`, `seq`, `max_ops` | `.collect()` from hexane decoders (lines 944–947), which don't report a length | `shrink_to_fit()` |
| `edges: Vec<Edge>` | `vec![]` then a push per dependency (line 984) | `shrink_to_fit()` |
| `seq_index[actor]` | one inner Vec per actor, grown by push (line 980) | `shrink_to_fit()` |

`parents` is already `Vec::with_capacity(len)` and is exact.

Measured with the patched copy (enabled by setting `RIGHTSIZE=1`):

| | C2 before | C2 after | C1 before | C1 after |
|---|---:|---:|---:|---:|
| `hashes` | 8.00 MiB | 4.10 | 4.00 | 2.84 |
| `edges` | 2.00 | 1.76 | 2.00 | 1.18 |
| `actors` + `seq` + `max_ops` | 3.00 | 1.53 | 1.50 | 1.08 |
| `seq_index` | 0.78 | 0.51 | 0.50 | 0.36 |
| **Whole document** | **30.31 MiB** | **24.45 MiB (−19%)** | **18.25** | **15.69 (−14%)** |

A1 (956 changes) barely changes. How much a document saves depends on where
its change count falls against a power of two: just past one, as with C2's
134,477 (2¹⁷ = 131,072), the Vecs are nearly half empty.

Caveats:

- **Spare capacity is room for future changes.** After shrinking, the next
  commit or merged change reallocates and copies each Vec once, after which
  doubling resumes. The clearest win is for documents that are mostly read
  after loading.
- **It doesn't touch the biggest field.** The empty half of `nodes_by_hash`
  comes from hashbrown's power-of-two buckets, and each hash is still stored
  twice. Changing that would need a different structure, for example a
  binary search over hashes sorted once at load.

## Peak memory while loading

### Not the rebuilt changes

Load rebuilds every change in its canonical encoded form so it can compute
the change hashes the `ChangeGraph` needs. That `Vec<Change>` is thrown away
afterwards, but it isn't what makes the peak: encoded changes are small, and
they aren't built until after the peak.

| Trace | File | Changes | Sum of encoded change bytes | Ops in changes |
|---|---:|---:|---:|---:|
| S1 | 342,625 B | 2 | 949,640 B | 779,335 |
| S2 | 601,801 B | 2 | 1,416,271 B | 1,104,628 |
| S3 | 832,481 B | 2 | 2,319,262 B | 2,339,472 |
| A1 | 787,154 B | 956 | 1,762,702 B | 947,338 |
| A2 | 578,946 B | 3,208 | 1,615,203 B | 695,971 |
| C1 | 141,590 B | 93,152 | 12,398,600 B | 651,951 |
| C2 | 213,016 B | 134,477 | 17,535,040 B | 608,151 |

The changes hold more ops than the document does. S3's document has 1.23M
ops, but its changes have 2.34M. A deletion is stored in the document only as
a `succ` mark on the deleted op, while a change holds it as an explicit
delete op, so load has to recreate about 1.1M delete ops for S3.

### What is on the heap at S3's peak (massif)

680.1 MiB of useful heap (713,161,052 B), matching the battery's peak:

| Allocation | MiB | Share |
|---|---:|---:|
| `BTreeMap<usize, OpBuilder>` re-sort queue, real ops (`ChangeBuilder::add`, `collector.rs:559` via `process_op_internal`) | 290.2 | 42.7% |
| The same queue, recreated delete ops (`flush_deletes`, `collector.rs:845`) | 267.5 | 39.3% |
| `IndexBuilder::new` scratch buffers (`op_set2/op_set/index.rs:166-167`) | 92.0 | 13.5% |
| Everything else (84 sites, each under 2%) | 30.4 | 4.5% |
| `Vec<Change>` | not visible | — |

- **Ops arrive in document order but must be grouped in op-ID order.** Ops
  that arrive before their turn are parked in
  `ProgressiveEncoder.queue: BTreeMap<usize, OpBuilder>` until the gap fills.
  S3 is one giant change, so nearly every op waits there.
- **Each queued op is a full `OpBuilder`:** op ID, object ID, key, value,
  flags, optional mark name and a heap `pred: Vec<OpId>`. Inside BTreeMap
  nodes that comes to about 250 B per op (massif bytes ÷ op count), against a
  byte or two per op in the hexane columns.
- **It is all freed before `load` returns,** which leaves only the compressed
  columns and the change graph described above.

## Hash memory, worked out

Each `ChangeHash` is 32 bytes. For 200,000 changes:

- `hashes` Vec: 200,000 × 32 B = 6.4 MB of data (more with spare capacity).
- `nodes_by_hash` keys: another 6.4 MB, plus the 4 B `NodeIdx` values and
  empty buckets, so about 8–13 MB.

That's roughly 15–20 MB of the 29.3 MB measured for 200k one-op changes.

## Files

| Path | What it is |
|---|---|
| `probe/` | Counting-allocator probes against the unmodified automerge 0.12.0 and yrs 0.28.1 crates. |
| `probe/src/bin/live.rs` | Steady and peak heap after `load` for every trace, both libraries (the *Battery numbers* table). Its "est graph" column assumes 150 B per change. |
| `probe/src/bin/actors.rs` | 600k ops with varying ops per change and number of actors. |
| `probe/src/bin/perchange.rs` | 200k ops with varying ops per change. |
| `probe/src/bin/chbytes.rs` | Encoded change bytes and op counts per trace. |
| `probe/src/bin/amload.rs` | Loads one Automerge fixture, for massif. |
| `graphprobe/` | Per-field `ChangeGraph` report built against a patched automerge 0.12.0. |
| `graphprobe/automerge-0.12.0-memory-report.patch` | Adds `ChangeGraph::mem_report`, `Automerge::probe_graph_mem` and `Automerge::probe_split`, plus the `RIGHTSIZE=1` experiment in `ChangeGraphCols::finalize`. |
| `graphprobe/setup.sh` | Copies automerge 0.12.0 from the cargo registry into `graphprobe/am-patched` and applies the patch. |

## Reproducing

```sh
D=../../../../benches/data/battery/egwalker-paper   # from inside probe/ or graphprobe/

cd probe
cargo +1.96.1 build --release
target/release/live $D            # battery steady/peak table
target/release/actors             # ops vs changes vs actors
target/release/perchange          # 200k ops, varying ops per change
target/release/chbytes $D         # encoded change bytes per trace
valgrind --tool=massif --threshold=2 --depth=40 --massif-out-file=ms.s3 \
  target/release/amload $D/S3.automerge
ms_print ms.s3 | less             # peak snapshot breakdown

cd ../graphprobe
./setup.sh
cargo +1.96.1 build --release
target/release/graphprobe $D C2 C1 A2               # per-field breakdown
RIGHTSIZE=1 target/release/graphprobe $D C2 C1 A1   # with right-sized Vecs
```

Massif needs one full load under valgrind (S3 takes about a minute).
