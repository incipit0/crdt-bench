# Port the Automerge benchmark battery: library-agnostic benchmarks

## Context

Automerge's `rust/benchmark-battery` has about 15 groups of benchmarks. Most of them only use plain text/list/map operations, so they can run against any CRDT library. This ticket ports those and builds the base that tickets 2 and 3 also need.

Source: `automerge/rust/benchmark-battery/` (scenarios in `src/scenarios/`, document generators in `src/lib.rs`).

## Part A: Base (do this first)

1. **Replace the `Crdt` trait** in `src/crdt.rs` with a generic trait that has an associated `Doc` type. The current `Rc<RefCell<dyn Crdt>>` + `Any` design only supports text and does dynamic dispatch inside timed loops.
   - Operations: text (insert/delete/splice/read), list, map (put/get/nested maps/len/iterate), save/load, incremental update (encode/apply), fork/merge, state-vector-style sync, marks, cursors.
   - An operation a library doesn't support returns `Unsupported` instead of panicking.
   - Adapters for Automerge and Yrs. Room for more later (Loro, diamond-types, cola).
2. **Bring over the battery's runner** (`runner.rs`, `report.rs`, `compare.rs`, `graph.rs`, `memory.rs`) in place of Criterion for these benchmarks:
   - Keep the three benchmark kinds: sampled, series, memory.
   - Name results `group/name/<lib>`.
   - Add an `unsupported` result status next to `not_run`.
   - Add a report that compares libraries within one run. The existing `compare` only compares two runs.
   - Have `scripts/bench.py` plot from the JSON output.
3. **Port the generators** from the battery's `src/lib.rs`, generic over the library: `poorly_simulated_typing_doc`, `big_paste_doc`, `big_random_doc`, `text_splice_100`, `list_splice_100`, `maps_in_maps_doc`, `deep_history_doc`. Use a seeded RNG everywhere. The battery uses an unseeded OS RNG in most places, so its documents differ between runs and between libraries.

## Part B: Benchmark groups

| Group | Benchmarks | Notes |
|---|---|---|
| `build` | all | Generators from Part A. |
| `load_save` | `load_*` / `save_*` for typing, big_paste, big_random, text_splice_100, maps_in_maps, deep_history; `load_actors/load/{10,100,1000}` | `load_actors/load_masked_half` is in ticket 2. |
| `apply` | `single_char_incremental_load` (series) | Automerge `save_incremental`/`load_incremental` corresponds to Yrs `encode_update_v1`/`apply_update`. |
| `typing` | all five series | Use the sync abstraction for `single_char_sync`. For Yrs, `apply_change`, `merge` and `incremental_load` all reduce to update exchange. Keep the variant names anyway. |
| `sync` | `full_many_tx`, `full_one_tx/*`, `every_change/*`, `tiny_text`, `tiny_list` | For Yrs, sync means a state-vector exchange followed by `encode_diff_v1`. This is not the same protocol as Automerge's sync, so record that in the report. `big_chunky_sync_message` is in ticket 2. |
| `map` | `repeated_put`, `increasing_put`, `decreasing_put` × {build, save, load, apply} × {100, 1k, 10k}; `deep_history/{n}` | `repeated_increment` is in ticket 2. For `apply`, replay each update into an empty doc. |
| `list` | `list_cursor_now`, `list_update_now`, `list_splice_index_now` | Yrs cursors use `StickyIndex`. The `_at` variants are in ticket 2. |
| `length` | `text_len_now`, `list_len_now`, `map_len_now` | The `_at` variants are in ticket 2. |
| `range` | `range/{10000,100000}` | The `_at` variants are in ticket 2. |
| `marks` | `add_mark`, `get_marks_at_end`, `splice_with_marks`, `splice_without_marks` | Yrs uses formatting attributes (`format`, `diff`/delta). Automerge's `ExpandMark::Both` has no exact Yrs equivalent, so record the closest setting used. |
| `edit_trace` | `edit_trace_single_tx`, `edit_trace_many_tx` | Replaces `benches/real.rs`'s sequential path. Use the battery's `data/edits.json`. |
| `egwalker_paper` | `load`, `get_text`, `clone` for S1, S3, A1, A2, C1, C2 | See below. `apply_fragment_to_clone` is in ticket 2. |
| `memory` | `load/typing`, `load/big-paste`, `load/text-splice-100`; egwalker `load`/`clone` | Generated docs only. The fixture-based memory benchmarks are in ticket 3. Uses the battery's tracking allocator behind a `memory` feature. |

### Eg-walker traces

The battery ships pre-built `.am` files. Each library needs its own pre-built document, so:

- Write a generic replayer for concurrent traces that supports N agents. A1 has 204 agents, A2 has 375 and C2 has 3. `load_conc` in `benches/real.rs` currently asserts `numAgents == 2`. The replayer forks from the parent version of each txn and merges when a txn has multiple parents.
- Add a `generate` command that writes `<trace>.<lib>` fixtures, like the battery's `egwalker-paper/generate.sh`.
- Check that each library's final text equals `endContent`.

## Done when

- `cargo run --release -- run` produces one JSON file with Automerge and Yrs results for every benchmark listed above.
- Every benchmark from tickets 2 and 3 appears as `not_run` or `unsupported`, not missing.
- The cross-library report and plots render.
- Every generated document's final state matches between the two libraries.
- README updated.
