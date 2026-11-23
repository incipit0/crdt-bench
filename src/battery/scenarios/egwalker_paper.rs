//! Loading, reading and copying documents built from the Eg-walker paper's
//! traces. Each library loads its own fixture, made by
//! `battery generate-fixtures`.
//!
//! `clone` measures the cheapest in-memory copy the library offers: a deep
//! clone for Automerge, an encode/decode round trip for Yrs (cloning a Yrs
//! `Doc` shares state).

use std::hint::black_box;

use crate::api::Crdt;
use crate::battery::registry::{Benchmark, BenchmarkKind, sampled, unrunnable};
use crate::battery::trace::TEXT;
use crate::battery::{EGWALKER_TRACES, egwalker_fixture_path};

pub fn register<D: Crdt>(out: &mut Vec<Benchmark>) {
    for label in EGWALKER_TRACES {
        let path = egwalker_fixture_path::<D>(label);
        let names = [
            format!("egwalker_paper/load/{label}"),
            format!("egwalker_paper/get_text/{label}"),
            format!("egwalker_paper/clone/{label}"),
        ];
        if !path.exists() {
            for name in names {
                out.push(unrunnable::<D>(
                    BenchmarkKind::Sampled,
                    "egwalker_paper",
                    name,
                    missing_fixture::<D>(label),
                ));
            }
            continue;
        }
        let [load, get_text, clone] = names;
        let bytes = move || std::fs::read(&path).expect("fixture is readable");
        let read = bytes.clone();
        out.push(sampled::<D>("egwalker_paper", load, move || {
            let bytes = read();
            Box::new(move || {
                black_box(D::load(&bytes));
            })
        }));
        let read = bytes.clone();
        out.push(sampled::<D>("egwalker_paper", get_text, move || {
            let doc = D::load(&read());
            Box::new(move || {
                let text = doc.get_text(TEXT).unwrap();
                black_box(doc.text(&text));
            })
        }));
        out.push(sampled::<D>("egwalker_paper", clone, move || {
            let doc = D::load(&bytes());
            Box::new(move || {
                black_box(doc.duplicate());
            })
        }));
    }
}

pub fn missing_fixture<D: Crdt>(label: &str) -> String {
    format!(
        "fixture missing: run `cargo run --release --bin battery -- generate-fixtures --library {} {label}`",
        D::NAME
    )
}
