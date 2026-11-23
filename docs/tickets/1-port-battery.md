# Port the Automerge benchmark battery: library-agnostic benchmarks

## Context

Automerge's `rust/benchmark-battery` has about 15 groups of benchmarks. Most of them only use plain text/list/map operations, so they can run against any CRDT library. This ticket ports those and builds the base that tickets 2 and 3 also need.

Source: `~/proj/automerge/rust/benchmark-battery/` (scenarios in `src/scenarios/`, document generators in `src/lib.rs`).

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
| `egwalker_paper` | `load`, `get_text`, `clone` for S1, S2, S3, A1, A2, C1, C2 | See below. The battery leaves out S2 without a stated reason; include it. `apply_fragment_to_clone` is in ticket 2. |
| `memory` | `load/typing`, `load/big-paste`, `load/text-splice-100`; egwalker `load`/`clone` | Generated docs only. The fixture-based memory benchmarks are in ticket 3. Uses the battery's tracking allocator behind a `memory` feature. |

### Eg-walker traces

The battery's `egwalker-paper/` directory has both the JSON traces and the `.am` files built from them. `generate.sh` runs `generate-egwalker-data`, which is `src/egwalker_data.rs`, to replay each JSON trace into Automerge. Each library needs its own pre-built document, so port that converter and make it generic over the library.

Keep what the battery's converter does:

- Check the trace before replaying it: every parent comes before its child, every `numChildren` is correct, there is at least one root txn and exactly one final txn.
- Support N agents. A1 has 204 agents, A2 has 375 and C2 has 3. `load_conc` in `benches/real.rs` currently asserts `numAgents == 2`.
- Keep one branch per txn, along with how many of its children have not yet used it. Each txn forks from its first parent and merges in the others. A2 has merges with up to 9 parents. When the last child uses a branch, move it instead of forking it.
- Use one fixed replica ID per agent so that fixtures are the same on every run. Automerge uses the agent's big-endian `u64` as the actor ID, and an extra root actor `0xff` creates the `"text"` object.
- Positions are counted in Unicode code points. Clamp positions and delete ranges that run past the end of the text, as the battery does.

Changes for Yrs:

- **ClientID per agent.** Yrs gives each item an ID made of `(ClientID, clock)`. Two concurrent branches that share a ClientID corrupt the document when they are merged. In A1, A2, C1 and C2, each agent's txns form a single chain, so it is safe to map every agent to one fixed ClientID. The replayer should still check this when it loads a trace (track a vector clock and confirm that each txn descends from all of its agent's earlier txns) and fail if the check doesn't hold.
- **Offsets.** Create docs with `OffsetKind::Utf32` so that Yrs indexes text in code points like the trace. The final texts are all ASCII at the moment, so a mistake here would not show up in the content check.
- **No root txn.** `get_or_insert_text("text")` replaces Automerge's `0xff` root txn.
- **Fork and merge.** Yrs has no `fork`. To fork, create a new `Doc` with the agent's ClientID and apply the parent's full state as an update. To merge, exchange state-vector diffs.

Also:

- Add a `generate` command that writes `<trace>.<lib>` fixtures for all seven traces (S1, S2, S3, A1, A2, C1, C2).
- Check the final text. For S1, S2 and S3 it must equal `endContent`. In the concurrent traces, each library orders concurrent inserts at the same position in its own way, so the final text may differ from `endContent`. The battery only prints a warning in that case. For A1, A2, C1 and C2, the libraries must produce the same text as each other, and any difference from `endContent` is reported but does not count as a failure.
  - **Revised:** A2 has five root transactions, so merging them creates concurrent inserts at the same position. That breaks the editing-traces rule that makes concurrent traces replay identically. Automerge and Yrs end with different texts (225,684 and 227,126 chars against 227,352), and the battery's `A2.am` has the same garbled text as our Automerge fixture. Decision: concurrent traces may differ from `endContent` and between libraries, as long as each library's length is within 1% of `endContent`'s. Sequential traces must still match exactly.

## Done when

- `cargo run --release -- run` produces one JSON file with Automerge and Yrs results for every benchmark listed above.
- Every benchmark from tickets 2 and 3 appears as `not_run` or `unsupported`, not missing.
- The cross-library report and plots render.
- Every generated document's final state matches between the two libraries.
- README updated.
