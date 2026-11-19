# Battery benchmarks that rely on Automerge-only features: decide how to simulate them

## Context

Some benchmarks in Automerge's `rust/benchmark-battery` use features that other CRDT libraries don't have, or don't expose in the same form. For each one we need to decide one of three things:

- (a) build an equivalent workload in other libraries,
- (b) build a workload that is close but measures something different, and label it that way, or
- (c) report it as `unsupported` for those libraries.

Depends on ticket 1, which provides the base and the `unsupported` result status.

## Benchmarks and candidate approaches (Yrs as the first non-Automerge library)

### Historical reads (`_at`)
- `length/text_len_at`, `length/map_len_at`, `range/range_at/{10000,100000}`, `list/list_cursor_at`
- Automerge reads any past version by its heads, without extra setup.
- Yrs candidate: create the doc with `skip_gc: true`, take a `Snapshot`, then either:
  - use `encode_state_from_snapshot` and load the result into a fresh doc, then read it, or
  - use the snapshot-aware reads that Yrs has (e.g. `Text::diff` with a snapshot).
- Question: should snapshot setup cost count in the timing? Automerge pays nothing up front, while Yrs pays for disabling GC and materialising the snapshot.

### Historical writes (`transaction_at`)
- `list/list_update_at`, `list/list_splice_index_at`
- Automerge creates a change whose dependencies are old heads, i.e. a concurrent branch.
- Yrs candidate: restore the snapshot into a fork, apply the edit there, then merge it back. This produces the same history, but the timing includes the fork and the merge.
- Question: is "fork at a version, edit, merge" an acceptable equivalent, or is it a different benchmark?

### Counters
- `map/*/repeated_increment/{100,1000,10000}` (build, save, load, apply)
- Yrs has no counter type.
- Candidates:
  - put the value + 1 (last-writer-wins). This is not equivalent under concurrency, but similar in cost for a single writer.
  - a counter built as a map of per-client values.
  - `unsupported`.

### Diff between versions
- `diff/diff`: `doc.diff(heads_a, heads_b)` between random points in the history of `essay.amrg`.
- Needs the `essay` fixture for each library (ticket 3), plus a way to diff between versions.
- Yrs candidate: `Text::diff` between two snapshots, with GC off. This covers text only and returns a different output (a delta, not patches).

### Automerge's sync wire format
- `sync/big_chunky_sync_message`: decode and receive the captured message `data/slowSyncMessage.amrgsync`.
- Candidate: for each library, capture a sync message or update of similar size, covering the same document change, then time decode + apply. This needs to know which document the message belongs to, so it overlaps with ticket 3.

### Automerge internals and loading options
- `egwalker_paper/apply_fragment_to_clone/*`: hand-builds a change so that it hits Automerge's fragment-boundary code path. This is specific to Automerge's design.
  - Candidate: replace it with a generic "apply one remote edit to a large doc loaded from a trace" benchmark that all libraries run, and keep the original as Automerge-only.
- `load_save/load_actors/load_masked_half/{10,100,1000}`: `LoadOptions::write_frontier`.
  - Probably `unsupported`. Need to check whether any other library has a similar option on load.

## Decide

- Reporting convention: should benchmarks of type (b) carry a visible marker in the report and plots? For example a `~` suffix or a separate "approximate" column.
- Each benchmark above gets labelled (a), (b) or (c), and the decision is recorded in the README.

## Done when

- Each benchmark above has a recorded decision.
- Every (a) and (b) is implemented for Yrs, with a test that both libraries reach the same final state.
- Every (c) appears as `unsupported` in the run output.
