use automerge::ReadDoc;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering::Relaxed};
use yrs::updates::decoder::Decode; use yrs::Transact;
struct C; static LIVE: AtomicIsize = AtomicIsize::new(0); static PEAK: AtomicIsize = AtomicIsize::new(0);
unsafe impl GlobalAlloc for C {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 { let v = LIVE.fetch_add(l.size() as isize, Relaxed) + l.size() as isize; PEAK.fetch_max(v, Relaxed); unsafe { System.alloc(l) } }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) { LIVE.fetch_sub(l.size() as isize, Relaxed); unsafe { System.dealloc(p, l) } }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 { let v = LIVE.fetch_add(n as isize - l.size() as isize, Relaxed) + n as isize - l.size() as isize; PEAK.fetch_max(v, Relaxed); unsafe { System.realloc(p, l, n) } }
}
#[global_allocator] static A: C = C;
fn mib(x: isize) -> f64 { x as f64 / 1048576.0 }
fn main() {
    let dir = std::env::args().nth(1).unwrap();
    for t in ["S1","S2","S3","A1","A2","C1","C2"] {
        let ab = std::fs::read(format!("{dir}/{t}.automerge")).unwrap();
        let yb = std::fs::read(format!("{dir}/{t}.yrs")).unwrap();
        let b0 = LIVE.load(Relaxed); PEAK.store(b0, Relaxed);
        let d = automerge::Automerge::load(&ab).unwrap();
        let (am, amp) = (LIVE.load(Relaxed) - b0, PEAK.load(Relaxed) - b0);
        let changes = d.stats().num_changes;
        drop(d);
        let b0 = LIVE.load(Relaxed); PEAK.store(b0, Relaxed);
        let y = yrs::Doc::new(); y.transact_mut().apply_update(yrs::Update::decode_v1(&yb).unwrap()).unwrap();
        let (ys, yp) = (LIVE.load(Relaxed) - b0, PEAK.load(Relaxed) - b0);
        drop(y);
        println!("{t}: AM steady {:6.2} MiB (peak {:6.1})  changes {:>6}  est graph {:5.1} MiB | Yrs steady {:5.2} MiB (peak {:5.1})",
            mib(am), mib(amp), changes, changes as f64 * 150.0 / 1048576.0, mib(ys), mib(yp));
    }
}
