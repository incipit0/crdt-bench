//! Is Automerge's commit cost per op or per byte?
//!
//! `cargo run --release --bin commit_cost`            timing experiments
//! `cargo run --release --bin commit_cost -- profile` one 100k paste + commit, for callgrind
use automerge::{AutoCommit, ObjType, ROOT, ReadDoc, transaction::Transactable};
use rand::{Rng, SeedableRng, distr::Alphanumeric, rngs::StdRng};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

const REPS: usize = 21;

fn text(n: usize) -> String {
    StdRng::seed_from_u64(1).sample_iter(&Alphanumeric).take(n).map(char::from).collect()
}

fn median(mut v: Vec<Duration>) -> f64 {
    v.sort();
    v[v.len() / 2].as_secs_f64() * 1e3
}

/// Returns (edit ms, commit ms, encoded change bytes, ops in change, sha256 ms over those bytes).
fn measure(n: usize, as_one_value: bool) -> (f64, f64, usize, usize, f64) {
    let s = text(n);
    let (mut edit, mut commit, mut hash) = (vec![], vec![], vec![]);
    let (mut bytes, mut ops) = (0, 0);
    for _ in 0..REPS {
        let mut doc = AutoCommit::new();
        let obj = doc.put_object(ROOT, "content", ObjType::Text).unwrap();
        doc.commit();
        let t0 = Instant::now();
        if as_one_value {
            doc.put(ROOT, "value", s.as_str()).unwrap(); // one op holding all n chars
        } else {
            doc.splice_text(&obj, 0, 0, &s).unwrap(); // n ops, one per char
        }
        let t1 = Instant::now();
        doc.commit();
        let t2 = Instant::now();
        let change = doc.get_last_local_change().unwrap();
        let raw = change.raw_bytes();
        bytes = raw.len();
        ops = change.len();
        let t3 = Instant::now();
        let digest = Sha256::digest(raw);
        std::hint::black_box(digest);
        hash.push(t3.elapsed());
        edit.push(t1 - t0);
        commit.push(t2 - t1);
    }
    (median(edit), median(commit), bytes, ops, median(hash))
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("profile") {
        let s = text(100_000);
        let mut doc = AutoCommit::new();
        let obj = doc.put_object(ROOT, "content", ObjType::Text).unwrap();
        doc.commit();
        doc.splice_text(&obj, 0, 0, &s).unwrap();
        doc.commit();
        assert_eq!(doc.length(&obj), 100_000);
        return;
    }
    println!("{:<34} {:>9} {:>10} {:>11} {:>9} {:>11}", "case", "edit ms", "commit ms", "change B", "ops", "sha256 ms");
    for n in [1_000, 10_000, 100_000] {
        let (e, c, b, o, h) = measure(n, false);
        println!("{:<34} {e:>9.3} {c:>10.3} {b:>11} {o:>9} {h:>11.4}", format!("splice {n} chars into text"));
    }
    for n in [100_000] {
        let (e, c, b, o, h) = measure(n, true);
        println!("{:<34} {e:>9.3} {c:>10.3} {b:>11} {o:>9} {h:>11.4}", format!("put one {n}-char string value"));
    }
}
