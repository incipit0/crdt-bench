//! Peak and steady heap use when loading (or copying) documents. Requires the
//! `memory` feature. The real-world Automerge fixtures are in `not_ported`
//! (ticket 3).

use std::path::PathBuf;

use crate::api::Crdt;
use crate::battery::generators::{big_paste_doc, poorly_simulated_typing_doc, text_splice_100};
use crate::battery::memory::measure;
use crate::battery::registry::{Benchmark, BenchmarkKind, memory, unrunnable};
use crate::battery::scenarios::egwalker_paper::missing_fixture;
use crate::battery::{EGWALKER_TRACES, egwalker_fixture_path};

const N: u64 = 100_000;

fn load<D: Crdt>(base: String, data: impl Fn() -> Vec<u8> + 'static) -> Benchmark {
    memory::<D>("memory", base, move || {
        let bytes = data();
        Box::new(move || measure(|| D::load(&bytes)))
    })
}

pub fn register<D: Crdt>(out: &mut Vec<Benchmark>) {
    let generated: [(&str, fn(u64) -> D); 3] = [
        ("typing", poorly_simulated_typing_doc::<D>),
        ("big-paste", big_paste_doc::<D>),
        ("text-splice-100", text_splice_100::<D>),
    ];
    for (label, build) in generated {
        out.push(load::<D>(format!("memory/load/{label}"), move || {
            build(N).save()
        }));
    }

    for label in EGWALKER_TRACES {
        let path: PathBuf = egwalker_fixture_path::<D>(label);
        let load_name = format!("memory/load/egwalker_paper/{label}");
        let clone_name = format!("memory/clone/egwalker_paper/{label}");
        if !path.exists() {
            for name in [load_name, clone_name] {
                out.push(unrunnable::<D>(
                    BenchmarkKind::Memory,
                    "memory",
                    name,
                    missing_fixture::<D>(label),
                ));
            }
            continue;
        }
        let read = path.clone();
        out.push(load::<D>(load_name, move || {
            std::fs::read(&read).expect("fixture is readable")
        }));
        out.push(memory::<D>("memory", clone_name, move || {
            let doc = D::load(&std::fs::read(&path).expect("fixture is readable"));
            Box::new(move || measure(|| doc.duplicate()))
        }));
    }
}
