use automerge::{transaction::Transactable, AutoCommit, Automerge, ObjType, ReadDoc, ROOT};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering::Relaxed};
struct C; static LIVE: AtomicIsize = AtomicIsize::new(0);
unsafe impl GlobalAlloc for C {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 { LIVE.fetch_add(l.size() as isize, Relaxed); unsafe { System.alloc(l) } }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) { LIVE.fetch_sub(l.size() as isize, Relaxed); unsafe { System.dealloc(p, l) } }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 { LIVE.fetch_add(n as isize - l.size() as isize, Relaxed); unsafe { System.realloc(p, l, n) } }
}
#[global_allocator] static A: C = C;

// n_ops chars typed, committed every `per` ops (per = ops per change)
fn build(n_ops: usize, per: usize) -> Vec<u8> {
    let mut d = AutoCommit::new();
    let t = d.put_object(ROOT, "text", ObjType::Text).unwrap();
    d.commit();
    for i in 0..n_ops {
        d.splice_text(&t, i, 0, "x").unwrap();
        if (i + 1) % per == 0 { d.commit(); }
    }
    d.save()
}
fn main() {
    let n = 200_000;
    println!("ops={n}");
    let mut base = 0isize;
    for per in [n, 1000, 100, 10, 1] {
        let bytes = build(n, per);
        let before = LIVE.load(Relaxed);
        let doc = Automerge::load(&bytes).unwrap();
        let live = LIVE.load(Relaxed) - before;
        let changes = doc.get_heads().len().max(1); let _ = changes;
        let nch = (n / per) + 1;
        if per == n { base = live; }
        println!("ops/change={per:>6} changes={nch:>7} file={:>8} B  resident={:>10} B  extra_vs_1change={:>10} B  per_change={:.0} B",
            bytes.len(), live, live - base, (live - base) as f64 / (nch as f64 - 2.0).max(1.0));
        drop(doc);
    }
}
