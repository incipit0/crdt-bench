use automerge::{transaction::Transactable, ActorId, AutoCommit, Automerge, ObjType, ROOT};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering::Relaxed};
struct C; static LIVE: AtomicIsize = AtomicIsize::new(0);
unsafe impl GlobalAlloc for C {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 { LIVE.fetch_add(l.size() as isize, Relaxed); unsafe { System.alloc(l) } }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) { LIVE.fetch_sub(l.size() as isize, Relaxed); unsafe { System.dealloc(p, l) } }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 { LIVE.fetch_add(n as isize - l.size() as isize, Relaxed); unsafe { System.realloc(p, l, n) } }
}
#[global_allocator] static A: C = C;
// n ops typed at the end; commit every `per` ops; rotate across `actors` actors per change
fn build(n: usize, per: usize, actors: usize) -> Vec<u8> {
    let ids: Vec<ActorId> = (0..actors).map(|i| ActorId::from(vec![i as u8 + 1; 16])).collect();
    let mut d = AutoCommit::new().with_actor(ids[0].clone());
    let t = d.put_object(ROOT, "text", ObjType::Text).unwrap();
    d.commit();
    let mut c = 0;
    for i in 0..n {
        d.splice_text(&t, i, 0, "x").unwrap();
        if (i + 1) % per == 0 { d.commit(); c += 1; d.set_actor(ids[c % actors].clone()); }
    }
    d.commit();
    d.save()
}
fn main() {
    let n = 600_000;
    let mut base = 0;
    for (per, actors) in [(n, 1), (4, 1), (4, 2), (4, 4)] {
        let b = build(n, per, actors);
        let before = LIVE.load(Relaxed);
        let doc = Automerge::load(&b).unwrap();
        let live = LIVE.load(Relaxed) - before;
        if per == n { base = live; }
        let ch = n / per;
        println!("ops={n} ops/change={per:>6} actors={actors} changes={ch:>6} file={:>8} B resident={:6.2} MiB  extra/change={:.0} B",
            b.len(), live as f64 / 1048576.0, (live - base) as f64 / ch.max(1) as f64);
        drop(doc);
    }
}
