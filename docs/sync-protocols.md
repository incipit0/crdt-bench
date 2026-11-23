# Sync protocols: y-sync and Automerge sync, by example

The battery's `sync` and `typing/single_char_sync` benchmarks sync two peers
with each library's own protocol (see the `sync` section of
[battery-benchmarks.md](battery-benchmarks.md)). This page runs the same small
scenario through both protocols and shows every message, so it's clear what
each `sync` call actually exchanges.

Both examples are real output, lightly reformatted, from the programs in
[`sync-examples/`](sync-examples/), run on 2026-11-23 with yrs 0.28.1 and
automerge 0.12.0.

## The scenario

1. Alice types `"Hello"` and the two peers sync, so both have `"Hello"`.
2. While offline, Alice appends `" world"`. Bob inserts `">> "` at the start,
   then deletes the `H` in a second transaction.
3. They sync. Both should end with `">> ello world"`.

## y-sync (Yrs)

y-sync is the protocol from Yjs's `y-protocols` package (yrs has the same
messages in its `sync` module). It's built on **state vectors**: a map from
client ID to the next clock that client expects. Each client numbers its
items 0, 1, 2…, so `{1: 5}` means "I have client 1's items with clocks 0–4".

| Message | Content | Reply |
|---|---|---|
| `SyncStep1` | My state vector | The receiver answers with `SyncStep2` |
| `SyncStep2` | `encode_diff(your state vector)`: everything you're missing | Applied with `apply_update`; no reply |
| `Update` | One local transaction's update, pushed to peers as it happens | Applied; no reply |

On connect both peers send `SyncStep1` and answer each other's with
`SyncStep2`: one round trip. The battery's Yrs adapter (`fn sync` in
`src/adapters/yrs.rs`) does exactly one such exchange per `sync` call, by
hand, through byte buffers.

In the example Alice is client 1 and Bob is client 2.

**1. After the initial sync**
```
alice: text="Hello"   state vector={1: 5}
bob  : text="Hello"   state vector={1: 5}
```
Bob hasn't typed anything, so he isn't in the state vector.

**2. After the offline edits**
```
alice: text="Hello world"   state vector={1: 11}
bob  : text=">> ello"       state vector={1: 5, 2: 3}
```
Alice's `" world"` got clocks 5–10 and Bob's `">> "` got clocks 0–2. Bob's
deletion doesn't change his state vector: deletions get no clocks and are
tracked separately in a **delete set**.

**3. `SyncStep1`: each peer sends its state vector**
```
alice → bob:  {1: 11}          (3 bytes)
bob → alice:  {1: 5, 2: 3}     (5 bytes)
```

**4. `SyncStep2`: each peer sends what the other lacks**
```
alice → bob (15 bytes):
  client 1: item <1#5>, len 6, origin-left <1#4>: ' world'
bob → alice (16 bytes):
  client 2: item <2#0>, len 3, origin-right <1#0>: '>> '
  delete set: client 1, clocks [0..1)        ← Alice's 'H'
```
- **Alice's diff:** Bob reported `1: 5`, so Alice sends her client-1 items
  from clock 5 onward. The six characters travel as one item.
- **Items say where they go by ID, not by position.** `' world'` goes right
  after `<1#4>` (the `o` in Hello), so the receiver attaches it next to that ID
  with no index arithmetic.
- **Bob's diff:** Alice's state vector has no entry for client 2, so Bob sends
  all his items. `'>> '` is anchored left of `<1#0>` (the `H`), and his
  deletion travels as a delete-set range.

**5. After both apply**
```
alice: text=">> ello world"   state vector={1: 11, 2: 3}
bob  : text=">> ello world"   state vector={1: 11, 2: 3}
```
They converge in one round trip and 39 bytes.

**6. Syncing again with nothing new**
```
alice → bob (6 bytes): delete set: client 1, clocks [0..1)
```
No items, because Bob's state vector covers everything Alice has. The delete
set is still sent: deletions have no clocks, so a state vector can't say "I
already know about this deletion". yrs resends the whole delete set every
time; applying it twice is harmless, and it's stored compactly as ID ranges.

## Automerge sync

Automerge identifies every change by its **hash** (SHA-256 of the change),
and each change lists the hashes of the changes it depends on. The protocol
is **stateful**: each peer keeps a `sync::State` per remote peer, recording
which heads (latest change hashes) they last had in common and what has
already been sent. Each message carries:

| Field | Meaning |
|---|---|
| `heads` | The sender's current heads |
| `need` | Hashes the sender is explicitly asking for |
| `have` | `last_sync` (heads at the last successful sync) plus a Bloom filter of the changes the sender has added since |
| `changes` | Change chunks for the receiver to apply |

The battery's Automerge adapter (`fn sync` in `src/adapters/automerge.rs`)
loops `generate_sync_message` / `receive_sync_message` in both directions
until neither side has anything to send.

Change labels in the output:

- **A0:** Alice creates the text and types `"Hello"`.
- **A1:** Alice appends `" world"`.
- **B1:** Bob inserts `">> "`.
- **B2:** Bob deletes the `H`.

**1. Initial sync** (Alice has A0, Bob has nothing)
```
msg 1 alice → bob    47 B  heads=[A0]                have={last_sync: [],   bloom: 5 B}
msg 2 bob → alice    42 B  heads=[]    need=[A0]     have={last_sync: [],   bloom: 0 B}
msg 3 alice → bob   196 B  heads=[A0]                changes: 1 chunk (147 B)  ← A0
msg 4 bob → alice    74 B  heads=[A0]                have={last_sync: [A0], bloom: 0 B}
```
- **msg 1 carries no changes.** Alice doesn't yet know what Bob has, so she
  only advertises her heads and a Bloom filter of her changes.
- **msg 2:** Bob sees a head he doesn't have and asks for it by hash.
- **msg 3:** Alice sends A0.
- **msg 4:** Bob confirms, and both record A0 as their last common point.

Four messages, two round trips.

**2. After the offline edits**
```
alice: "Hello world"   heads=[A1]
bob  : ">> ello"       heads=[B2]
```

**3. Sync again, reusing each side's sync state**
```
msg 1 alice → bob   239 B  heads=[A1]      have={last_sync: [A0],     bloom: 5 B}  changes: [158 B] ← A1
msg 2 bob → alice   358 B  heads=[B2, A1]  have={last_sync: [A1],     bloom: 6 B}  changes: [244 B] ← B1+B2
msg 3 alice → bob   138 B  heads=[B2, A1]  have={last_sync: [B2, A1], bloom: 0 B}
alice: ">> ello world"   heads=[B2, A1]
bob  : ">> ello world"   heads=[B2, A1]
```
- **msg 1 already includes A1.** Alice's sync state remembers that Bob had
  A0, so she sends everything after A0 without being asked.
- **msg 2:** Bob has applied A1. From Alice's Bloom filter he works out that
  she lacks B1 and B2, and sends both in one chunk.
- **msg 3:** Alice confirms. The heads `[B2, A1]` are two concurrent branches;
  they stay as two heads until a later change depends on both.

**4. Syncing again with nothing new:** no messages at all, because both sync
states already record the other side as up to date.

## Side by side: the concurrent-edit sync

| | y-sync (Yrs) | Automerge sync |
|---|---|---|
| What identifies "what I have" | State vector, e.g. `{1: 5, 2: 3}` | Heads (32-byte hashes) plus a Bloom filter of changes since the last sync |
| Per-peer state between syncs | None | `sync::State` (shared heads, what was sent) |
| Messages | 2 state vectors + 2 diffs, one round trip | 3 messages, 1.5 round trips (4 messages, 2 round trips with no prior state) |
| Bytes on the wire | 39 B | 735 B |
| How data travels | Items anchored to neighbour IDs, plus a delete set | Whole change chunks: actor, seq, op range, dependency hashes, ops in columns |
| Re-sync with nothing new | 6 B (the delete set is always resent) | 0 messages |

- **Most of Automerge's extra bytes are hashes.** Every message carries heads,
  every `have` carries `last_sync` hashes, and every change chunk carries its
  dependencies' hashes, at 32 bytes each. In a tiny example that dominates the
  payload.
- **The Bloom filter is why `need` exists.** A false positive can make a peer
  think the other already has a change. When the other side then sees a head
  it can't resolve, it asks for the missing hash explicitly, at the cost of an
  extra round trip. That didn't happen here.
- **What each design buys.** A state vector only works if every client numbers
  its items without gaps and peers trust those numbers. Automerge's hashes
  don't need that, and they give a tamper-evident history, at the price of
  bigger messages and per-peer sync state.

## Running the examples

```sh
cd docs/sync-examples
cargo run --bin ysync    # y-sync with yrs 0.28.1
cargo run --bin amsync   # Automerge sync with automerge 0.12.0
```

Both programs are deterministic, and the Automerge one prints change hashes
as labels (A0, A1, B1, B2).
