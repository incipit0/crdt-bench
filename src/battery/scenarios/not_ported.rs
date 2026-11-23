//! Benchmarks from the Automerge battery that aren't ported yet. They are
//! registered so every run lists them, with the reason, instead of silently
//! leaving them out.

use crate::api::Crdt;
use crate::battery::registry::{Benchmark, BenchmarkKind, unrunnable};
use crate::battery::scenarios::map::SIZES;

pub const MISSING_DATA: &str =
    "input data missing: run scripts/fetch-battery-data.sh (or set CRDT_BATTERY_DATA)";

const TICKET_2: &str = "not ported yet: needs an Automerge-only feature (docs/tickets/2-automerge-only.md)";
const TICKET_3: &str = "not ported yet: needs real-world fixtures for every library (docs/tickets/3-fixtures.md)";

const BESTIARY: [&str; 2] = ["godot-modeled", "godot-thread"];
const FIXTURES: [&str; 7] = [
    "unsinkable",
    "moby-dick",
    "essay",
    "stephen",
    "webstraits",
    "monday-meeting-notes",
    "pathological",
];

pub fn register<D: Crdt>(out: &mut Vec<Benchmark>) {
    let mut add = |kind, group, name: String, reason: &str| {
        out.push(unrunnable::<D>(kind, group, name, reason));
    };
    use BenchmarkKind::{Memory, Sampled};

    // Ticket 2: features only Automerge has.
    for name in [
        "length/text_len_at",
        "length/map_len_at",
        "list/list_cursor_at",
        "list/list_update_at",
        "list/list_splice_index_at",
    ] {
        let group = name.split('/').next().unwrap();
        add(Sampled, group, name.to_string(), TICKET_2);
    }
    for n in [10_000, 100_000] {
        add(Sampled, "range", format!("range/range_at/{n}"), TICKET_2);
    }
    for n in SIZES {
        add(Sampled, "map", format!("map/repeated_increment/{n}"), TICKET_2);
        for op in ["save", "load", "apply"] {
            add(
                Sampled,
                "map",
                format!("map/{op}/repeated_increment/{n}"),
                TICKET_2,
            );
        }
    }
    for n in [10, 100, 1000] {
        add(
            Sampled,
            "load_save",
            format!("load_save/load_actors/load_masked_half/{n}"),
            TICKET_2,
        );
    }
    add(
        Sampled,
        "sync",
        "sync/big_chunky_sync_message".to_string(),
        TICKET_2,
    );
    for label in crate::battery::EGWALKER_TRACES {
        add(
            Sampled,
            "egwalker_paper",
            format!("egwalker_paper/apply_fragment_to_clone/{label}"),
            TICKET_2,
        );
    }

    // Ticket 3: real-world Automerge documents.
    add(Sampled, "diff", "diff/diff".to_string(), TICKET_3);
    for fixture in BESTIARY {
        for op in ["load", "reload", "fork", "save", "iter"] {
            add(
                Sampled,
                "bestiary",
                format!("bestiary/{op}/{fixture}"),
                TICKET_3,
            );
        }
    }
    for fixture in FIXTURES {
        add(Memory, "memory", format!("memory/load/{fixture}"), TICKET_3);
    }
}
