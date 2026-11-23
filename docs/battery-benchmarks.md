# Benchmark battery: what each benchmark measures

This is a reference for `src/bin/battery.rs`, the cross-library port of
Automerge's `rust/benchmark-battery`. Each workload is written once in
`src/battery/scenarios/<group>.rs` and run once per library. The result names
are `<group>/<case>/<library>`, so `typing/single_char_merge/yrs` is the
`single_char_merge` case of the `typing` group, run with Yrs.

## How to read the results

| Kind | Unit | What a number means |
|---|---|---|
| sampled | ns/iter | Median time of one iteration over the measured samples. The report also shows min and max. |
| series | ns/step | Time of each step in a sequence of steps against one document that keeps growing. The report shows the total, median and last-window median, and plots every step. |
| memory | bytes | Heap allocated on the current thread by one operation. **Peak** is the high-water mark during the operation; **steady** is what is still allocated afterwards, while the result is still alive. This needs `--features memory`. |

- **Setup is untimed.** Building the input document, loading a fixture, and
  (for the benchmarks that need a fresh input every iteration) copying the
  source document all happen outside the timer. Each section below says what
  is inside it.
- **Whether freeing the result is timed depends on the benchmark.** In most
  sampled benchmarks (`build`, `load_save`, `map` build/save/load,
  `egwalker_paper`) the result is dropped inside the timed iteration, so
  `build` and `load` include freeing the document. The benchmarks that rebuild
  their input every iteration (`sync/full_*`, `marks/splice_*`, `map/apply/*`
  and `edit_trace`) hand their result back and it is dropped outside the
  timer.
- **Some benchmarks keep their state between iterations.** Where a benchmark
  edits a document in place, the next iteration sees the edited document.
  This applies to `list_update_now`, `list_splice_index_now` and `add_mark`.
  The changes are small next to the 100k-element document.
- **Tiers.** ⚡ marks the cases in the fast tier, which is what `run` runs by
  default. `--tier all` adds the rest.
- **Same input for every library.** Generated documents use a fixed RNG seed,
  so every library gets the same edits on every run.

### Library differences that apply everywhere

| Topic | Automerge | Yrs |
|---|---|---|
| Text positions | Unicode code points, natively | The adapter converts code points to UTF-16. It skips the conversion when the text has no characters outside the BMP. |
| One transaction | One `AutoCommit` commit, which is one change | One `TransactionMut` |
| Copying a document (`fork`, `duplicate`) | `fork()` or `clone()` | Encode the whole state, then decode it into a new `Doc`. Cloning a Yrs `Doc` would share its state instead of copying it. |
| Incremental save and load | `save_incremental` / `load_incremental` | `encode_diff_v1` against the state vector at the last save / `apply_update` |
| Sync | The Automerge sync protocol, run until both peers are idle. Every message is encoded and decoded. | One y-sync round: send the state vector, get the diff back. Bytes are encoded the same way. |
| Change history | One change per transaction | No per-transaction history. `changes()` returns a single update holding the whole document. |

## Shared inputs

### Generated documents (`src/battery/generators.rs`)

Most groups build their documents with these generators. N is 100,000 unless a
benchmark says otherwise.

| Generator | Document |
|---|---|
| `poorly_simulated_typing_doc` ("typing") | N single-character appends to a text, **one transaction each**: a document with a long history. |
| `big_paste_doc` ("big_paste") | One N-character insert in one transaction: a large text with a tiny history. |
| `big_random_doc` ("big_random") | N single characters inserted at random positions, in one transaction. |
| `text_splice_100` | N/100 inserts of 100 characters each, in one transaction. As in the original battery, positions are drawn from the number of splices so far rather than the number of characters, so inserts cluster near the start. |
| `list_splice_100` | The same as `text_splice_100`, but into a list of one-character strings. |
| `maps_in_maps_doc` | N maps, each nested inside the previous one, in one transaction. |
| `deep_history_doc` | N transactions, each overwriting root keys `x` and `y`: a tiny document with a long history. |
| `random_insert_text_doc` | A text built from 1-character inserts at random positions in one transaction. The `apply` and `typing` groups use it as their 100k-character starting text. |
| `string_map_doc` | A root map with N string keys and string values, in one transaction. |

### Editing traces

| Input | Used by | Contents |
|---|---|---|
| `edits.json` | `edit_trace` | The battery's sequential trace: 259,778 edits (182,315 with an insert, 77,463 with a delete) in `[pos, del, ...chars]` form. |
| `egwalker-paper/<trace>.json` | `egwalker_paper`, `memory` | The Eg-walker paper's traces, replayed once into a fixture per library by `generate-fixtures` (see the README). |

| Trace | Txns | Agents | Final text (chars) | Notes |
|---|---:|---:|---:|---|
| S1, S2, S3 | 1 each | 1 | 314,556 / 170,307 / 122,271 | Sequential: one user typing. |
| A1 | 955 | 204 | 38,142 | Asynchronous: the git history of a node.js source file (`node_nodecc`), from `dt git-import`. |
| A2 | 3,210 | 375 | 227,352 expected | Asynchronous: git's `Makefile` history. It has five root histories, so each library ends with slightly different, partly garbled text, and so do the paper's own files. |
| C1 | 93,151 | 2 | 534,050 | Concurrent: two users typing live (`friendsforever`), duplicated 25×. |
| C2 | 134,476 | 3 | 528,700 | Concurrent: live collaborative typing (`clownschool`). |

## Groups

### `build`: create a document from scratch

Each case times a full generator run with N = 100,000: create an empty
document, apply every edit, and free the finished document.

| Case | What is timed |
|---|---|
| `build_typing` | 100k single-character appends, one transaction each. |
| `build_big_paste` ⚡ | One 100k-character insert. |
| `build_text_splice_100` ⚡ | 1,000 inserts of 100 characters, one transaction. |
| `build_maps_in_maps_doc` | 100k nested map creations, one transaction. |
| `build_deep_history_doc` | 100k transactions overwriting two keys. |

### `load_save`: serialize and deserialize

For each generated document (N = 100k), `save_<doc>` times encoding the whole
document and `load_<doc>` times decoding it. The document or its bytes are
prepared outside the timer.

| Case | Document |
|---|---|
| `load_typing`, `save_typing` ⚡ | typing (100k one-character transactions) |
| `load_big_paste` ⚡, `save_big_paste` ⚡ | big_paste |
| `load_text_splice_100` ⚡, `save_text_splice_100` ⚡ | text_splice_100 |
| `load_big_random`, `save_big_random` | big_random |
| `load_maps_in_maps`, `save_maps_in_maps` | maps_in_maps |
| `load_deep_history/1000`, `save_deep_history/1000` | deep_history with N = 100k. The `/1000` suffix is a leftover name from the original battery. |
| `load_actors/load/{10,100,1000}` | Load a document in which each of n agents wrote one key and was merged in, so it has many actors (Automerge) or client IDs (Yrs). |

### `apply`: incremental updates to a replica (series)

| Case | Steps | What each step does |
|---|---:|---|
| `single_char_incremental_load` | 1,000 | Insert `.` at a random position in a 100k-character text, `save_incremental` it, and `load_incremental` it into a forked replica. Then check that both have the same version. |

### `typing`: one keystroke reaching a replica (series)

Every case starts from a 100k-character text and a fork of it as the replica.
Each step types `.` at a random position and gets it to the replica in a
different way. For Yrs, every variant comes down to exchanging updates, but
the names are kept so the results line up with Automerge's.

| Case | Steps | How the edit reaches the replica |
|---|---:|---|
| `single_char_sync` | 1,000 | The sync protocol (Automerge sync / one y-sync round). |
| `single_char_apply_change` | 1,000 | Take the last local change and apply it. Yrs records each transaction's update for this. |
| `single_char_merge` | 1,000 | `merge`: Automerge's change exchange / a Yrs state-vector diff. |
| `single_char_100_bulk_incremental_load` ⚡ | 100 | 100 keystrokes in **one** transaction, then one incremental save and load. |
| `single_char_100_incremental_loads` | 100 | 100 keystrokes, each in its own transaction with its own incremental save and load. |

### `sync`: two peers syncing

The peers' documents are built outside the timer. In the `full_*` cases a
fresh copy of the source is made for every iteration, also outside the timer.

| Case | Kind | What is timed |
|---|---|---|
| `full_many_tx` ⚡ | sampled | Sync a 10k-key map, written one key per transaction, to an empty peer. |
| `full_one_tx/100` ⚡, `/1000` ⚡, `full_one_tx` ⚡ | sampled | The same, with 100, 1k or 10k keys written in one transaction. The unsized name is 10k, as in the original battery. |
| `every_change/{100,1000,10000}` (100 ⚡) | sampled | Write n keys one transaction at a time, syncing to the other peer after each. Everything is timed, including creating both peers. |
| `tiny_text` | series, 1,000 steps | A 100k text already synced to a peer. Each step replaces one character at a random position, then syncs. |
| `tiny_list` | series, 1,000 steps | A 100k list already synced to a peer. Each step inserts `"_"` at a random position, then syncs. |

### `map`: root-map workloads at 100, 1k and 10k keys

There are three ways of filling the map, all in one transaction:

- `repeated_put`: n puts to the same key `"0"`, so one live value and n − 1 overwritten.
- `increasing_put`: keys `0, 1, …, n−1`.
- `decreasing_put`: keys `n−1, …, 0`.

Each one is measured four ways, for n in {100, 1000, 10000}. Only the
n = 1000 build cases are in the fast tier: `repeated_put/1000`,
`increasing_put/1000`, `decreasing_put/1000` and `deep_history/1000`.

| Case | What is timed |
|---|---|
| `map/<op>/{n}` | Build the map. |
| `map/save/<op>/{n}` | Save it. |
| `map/load/<op>/{n}` | Load it from saved bytes. |
| `map/apply/<op>/{n}` | Apply all its changes to an empty document. The map was built in one transaction, so this is one change in both libraries. |
| `map/deep_history/{n}` | Build `deep_history_doc(n)`: n transactions overwriting `x` and `y`. |

### `list`: one operation on a 100k list

The list is built by `list_splice_100`. Each iteration picks a random index.

| Case | What is timed |
|---|---|
| `list_cursor_now` ⚡ | Create a cursor at the index and resolve it back to a position. In Yrs that's a `StickyIndex` with `Assoc::After`. |
| `list_update_now` ⚡ | Replace the element at the index. Yrs has no in-place set, so it's a delete plus an insert. |
| `list_splice_index_now` ⚡ | Insert one element at the index. |

### `length`: size query at the current version

| Case | What is timed |
|---|---|
| `text_len_now` ⚡ | Length of a 100k text (text_splice_100), in code points. |
| `list_len_now` ⚡ | Length of a 100k list (list_splice_100). |
| `map_len_now` ⚡ | Number of keys in a 100k-key map. |

### `range`: iterate a map

| Case | What is timed |
|---|---|
| `range/{10000 ⚡, 100000}` | Visit every value of an n-key map. |

### `marks`: rich-text formatting on a 100k text

Automerge marks use `ExpandMark::Both`. Yrs formatting attributes have no
expand setting: text inserted at a mark's edge takes the formatting of the
character before it.

| Case | What is timed |
|---|---|
| `add_mark` ⚡ | Add one boolean mark over a random range. Marks build up across iterations. |
| `get_marks_at_end` | Count the marks on the last character of a text that has 10 random marks. Yrs has no point query, so the adapter walks the text's formatting delta. |
| `splice_with_marks` | 1,000 transactions, each inserting `XXX` at a random position, into a copy of a text with 1,000 random marks. |
| `splice_without_marks` ⚡ | The same with no marks, and each splice also deletes one character. That's how the original battery defines it, so the two splice cases aren't exactly the same workload. |

### `edit_trace`: replay a real editing session

Replays `edits.json` (259,778 edits) into an empty text.

| Case | What is timed |
|---|---|
| `edit_trace_single_tx` | All edits in **one** transaction. In the 2026-11-19 run, Yrs took 126 s here, 19× its own `many_tx` time and 38× Automerge's 3.3 s. Yrs probably merges adjacent items only when a transaction commits, so a 260k-edit transaction keeps every keystroke as its own item. |
| `edit_trace_many_tx` | One transaction per edit. |

### `egwalker_paper`: documents built from the Eg-walker paper's traces

Each library loads its own fixture, built by `generate-fixtures` (see
*Editing traces* above), for S1, S2, S3, A1, A2, C1 and C2.

| Case | What is timed |
|---|---|
| `load/<trace>` | Decode the saved fixture into a document. |
| `get_text/<trace>` | Read the whole text from a loaded document. Only A1 is in the fast tier ⚡. |
| `clone/<trace>` | The cheapest in-memory copy: a deep clone in Automerge, encode and decode in Yrs. |

### `memory`: heap use (requires `--features memory`)

| Case | What is measured |
|---|---|
| `memory/load/{typing, big-paste, text-splice-100}` | Peak and steady heap while loading the generated document (N = 100k). |
| `memory/load/egwalker_paper/<trace>` | The same, for each Eg-walker fixture. |
| `memory/clone/egwalker_paper/<trace>` | Peak and steady heap of copying a loaded fixture. |

## Registered but not run yet

These benchmarks appear in every result file as `not_run`, with a reason, so
they don't vanish silently:

- **Ticket 2 (Automerge-only features):**
  - `_at` variants that query a historical version: `length/*_len_at`, `list/*_at`, `range/range_at/*`
  - `map/*repeated_increment*`, which needs a counter type
  - `load_save/load_actors/load_masked_half/*`
  - `sync/big_chunky_sync_message`
  - `egwalker_paper/apply_fragment_to_clone/*`
- **Ticket 3 (needs every library's version of Automerge's real-world documents):**
  - `diff/diff`
  - `bestiary/*`
  - `memory/load/<fixture>` for unsinkable, moby-dick, essay, stephen, webstraits, monday-meeting-notes and pathological

See `docs/tickets/2-automerge-only.md` and `docs/tickets/3-fixtures.md`.
