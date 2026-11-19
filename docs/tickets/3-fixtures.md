# Make the battery's real-world Automerge fixtures available to other CRDT libraries

## Context

Several benchmarks in Automerge's `rust/benchmark-battery` load real documents that are stored in Automerge's binary format. Other libraries can't read these files, so we need an equivalent document for each library, with the same content and history.

Depends on ticket 1, which provides the base and the generic replayer for concurrent traces.

## Fixtures and the benchmarks that need them

| Fixture (`benchmark-battery/data/`) | Size | Used by |
|---|---|---|
| `godot-modeled.amrg` | 85 KB | `bestiary/{load,reload,fork,save,iter}` |
| `godot-thread.amrg` | 3.3 MB | `bestiary/{load,reload,fork,save,iter}` |
| `unsinkable.amrg` | 19.6 MB | `memory/load` |
| `moby-dick.automerge` | 0.5 MB | `memory/load` |
| `essay.amrg` | 0.9 MB | `memory/load`, `diff/diff` (see ticket 2) |
| `stephen.amrg` | 0.2 MB | `memory/load` |
| `webstraits.amrg` | 0.2 MB | `memory/load` |
| `monday-meeting-notes.automerge` | 0.2 MB | `memory/load` |
| `pathological.amrg` | 28.5 MB | `memory/load` |
| `slowSyncMessage.amrgsync` | 0.85 MB | `sync/big_chunky_sync_message` (see ticket 2) |

## Proposed approach

1. **Inventory.** For each fixture, record:
   - which object types it uses: text only, or nested maps/lists, marks, counters;
   - how many actors and changes it has, and how much concurrency (the largest number of heads at once).

   This tells us which converter is needed for which fixture.
2. **Export from Automerge to a library-neutral trace.**
   - Walk the change graph in topological order. For each change, get the patches with `diff(parent heads, change heads)`.
   - Write the result in a neutral format:
     - **Text-only fixtures:** reuse the Eg-walker / editing-traces JSON format (`txns` with `parents`, `agent`, `patches`), so the replayer from ticket 1 works without changes.
     - **Structured fixtures (godot, maybe others):** add a richer op format with map put/delete, list insert/delete, nested object creation, marks and counters. Counters depend on what ticket 2 decides.
   - Keep the original actors as agents, so that concurrency and the number of authors are preserved.
3. **Import into each library and save** a `<fixture>.<lib>` file, using the same `generate` command as the Eg-walker traces.
4. **Check the result.** The materialized JSON state of each generated doc must equal Automerge's state at the final heads.

## Open questions

- **Exact equivalence isn't possible.** Each library will compress and garbage-collect history differently. That's fine, because it's what we are measuring, but the README should say so.
- **Storage.** `pathological` and `unsinkable` are about 48 MB together, before adding a copy for each library. Options: commit them (Git LFS?), or generate them at bench time from the neutral trace and cache the result outside git. Note that `benches/data/editing-traces` is currently being removed from tracking, so decide this together with that.
- **`bestiary/iter`** iterates the whole document. Define "iterate" for each library: a full JSON materialisation, or walking every op/item.

## Done when

- A neutral trace is committed or generated for every fixture that can be converted, with a list of any that can't and why.
- A Yrs fixture exists for each of those, and its final state matches Automerge's.
- `bestiary/*` and fixture-based `memory/load/*` run for Yrs.
