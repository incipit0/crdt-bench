use automerge::{Automerge, ReadDoc};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering::Relaxed};
struct C; static LIVE: AtomicIsize = AtomicIsize::new(0);
unsafe impl GlobalAlloc for C {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 { LIVE.fetch_add(l.size() as isize, Relaxed); unsafe { System.alloc(l) } }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) { LIVE.fetch_sub(l.size() as isize, Relaxed); unsafe { System.dealloc(p, l) } }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 { LIVE.fetch_add(n as isize - l.size() as isize, Relaxed); unsafe { System.realloc(p, l, n) } }
}
#[global_allocator] static A: C = C;
fn measure(f: &mut dyn FnMut()) -> isize { let b = LIVE.load(Relaxed); f(); LIVE.load(Relaxed) - b }
fn mib(x: isize) -> f64 { x as f64 / 1048576.0 }
fn main() {
    let dir = std::env::args().nth(1).unwrap();
    for t in std::env::args().skip(2) {
        let b = std::fs::read(format!("{dir}/{t}.automerge")).unwrap();
        let before = LIVE.load(Relaxed);
        let d = Automerge::load(&b).unwrap();
        let total = LIVE.load(Relaxed) - before;
        let n = d.stats().num_changes;
        println!("== {t}: resident after load {:.2} MiB, {n} changes, {} actors", mib(total), d.stats().num_actors);
        for (k, v) in d.probe_split(&measure) { println!("   split  {k:<24} {:7.2} MiB", mib(v)); }
        let rep = d.probe_graph_mem(&measure);
        let sum: isize = rep.iter().map(|x| x.2).sum();
        println!("   graph fields sum {:.2} MiB = {:.0} B/change", mib(sum), sum as f64 / n as f64);
        for (k, len, bytes) in rep { if bytes > 0 {
            println!("     {k:<46} len={len:>7}  {:7.2} MiB  {:5.1}%  {:6.1} B/change", mib(bytes), 100.0 * bytes as f64 / sum as f64, bytes as f64 / n as f64);
        }}
    }
}
