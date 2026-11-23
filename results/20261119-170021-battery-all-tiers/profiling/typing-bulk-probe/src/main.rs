use crdt_bench::adapters::{automerge::AmDoc, yrs::YrsDoc};
use crdt_bench::api::{Crdt, Txn};
use crdt_bench::battery::generators::{index, random_insert_text_doc, rng_with, CONTENT};
use std::time::{Duration, Instant};

fn run<D: Crdt>(steps: usize) {
    let mut rng = rng_with(4);
    let mut doc: D = random_insert_text_doc(100_000, 1, &mut rng);
    let mut remote = doc.fork();
    doc.save_incremental();
    let text = doc.get_text(CONTENT).unwrap();
    let (mut splice, mut commit, mut save, mut load, mut check) = (Duration::ZERO, Duration::ZERO, Duration::ZERO, Duration::ZERO, Duration::ZERO);
    for _ in 0..steps {
        let t = Instant::now();
        let mut tx = doc.transact();
        for _ in 0..100 { tx.splice_text(&text, index(&mut rng, 100_000), 0, "."); }
        splice += t.elapsed();
        let t = Instant::now(); drop(tx); commit += t.elapsed();
        let t = Instant::now(); let bytes = doc.save_incremental(); save += t.elapsed();
        let t = Instant::now(); remote.load_incremental(&bytes); load += t.elapsed();
        let t = Instant::now(); assert!(doc.same_version(&mut remote)); check += t.elapsed();
    }
    let ms = |d: Duration| d.as_secs_f64() * 1e3 / steps as f64;
    println!("{:>9}: per step  100 splices {:7.2} ms ({:5.1} us each) | commit {:6.2} | save_incremental {:6.2} | load_incremental {:6.2} | same_version {:5.2} | total {:7.2} ms",
        D::NAME, ms(splice), ms(splice) * 10.0, ms(commit), ms(save), ms(load), ms(check), ms(splice + commit + save + load + check));
}
fn by_pos<D: Crdt>() {
    let mut rng = rng_with(4);
    let mut doc: D = random_insert_text_doc(100_000, 1, &mut rng);
    let text = doc.get_text(CONTENT).unwrap();
    print!("{:>9}: us per insert at position", D::NAME);
    for p in [0usize, 1_000, 25_000, 50_000, 75_000, 99_000] {
        let n = 200;
        let t = Instant::now();
        for _ in 0..n { doc.transact().splice_text(&text, p, 0, "."); }
        print!(" | {p:>6}: {:7.1}", t.elapsed().as_secs_f64() * 1e6 / n as f64);
    }
    println!();
}
fn main() {
    if std::env::args().nth(1).as_deref() == Some("pos") { by_pos::<AmDoc>(); by_pos::<YrsDoc>(); return; }
    match std::env::args().nth(1).as_deref() {
        Some("am") => run::<AmDoc>(3),
        Some("yrs") => run::<YrsDoc>(3),
        _ => { run::<AmDoc>(20); run::<YrsDoc>(20); }
    }
}
