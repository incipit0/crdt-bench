//! Replaying the battery's sequential editing trace (`edits.json`).

use std::rc::Rc;

use crate::api::{Crdt, Txn};
use crate::battery::registry::{Benchmark, BenchmarkKind, rebuilt, unrunnable};
use crate::battery::trace::{Edit, TEXT, load_edits};
use crate::battery::{edits_path, scenarios::not_ported::MISSING_DATA};

fn benchmark<D: Crdt>(base: &'static str, one_tx: bool) -> Benchmark {
    let path = edits_path();
    if !path.exists() {
        return unrunnable::<D>(BenchmarkKind::Sampled, "edit_trace", base, MISSING_DATA);
    }
    rebuilt::<D, _, _, _, _, _>(
        "edit_trace",
        base,
        move || {
            let edits = Rc::new(load_edits(&path).expect("edits.json parses"));
            move || {
                let mut doc = D::new();
                let text = doc.transact().create_text(TEXT);
                (edits.clone(), doc, text)
            }
        },
        move |(edits, mut doc, text): (Rc<Vec<Edit>>, D, D::Text)| {
            if one_tx {
                let mut tx = doc.transact();
                for edit in edits.iter() {
                    tx.splice_text(&text, edit.pos, edit.delete, &edit.insert);
                }
            } else {
                for edit in edits.iter() {
                    doc.transact()
                        .splice_text(&text, edit.pos, edit.delete, &edit.insert);
                }
            }
            doc
        },
    )
}

pub fn register<D: Crdt>(out: &mut Vec<Benchmark>) {
    out.push(benchmark::<D>("edit_trace/edit_trace_single_tx", true));
    out.push(benchmark::<D>("edit_trace/edit_trace_many_tx", false));
}
