//! Benchmark groups. Each module mirrors the Automerge battery's module of the
//! same name and registers its benchmarks for one library at a time.

use crate::api::Crdt;
use crate::battery::registry::Benchmark;

mod apply;
mod build;
mod edit_trace;
mod egwalker_paper;
mod length;
mod list;
mod load_save;
mod map;
mod marks;
mod memory;
mod not_ported;
mod range;
mod sync;
mod typing;

pub fn register<D: Crdt>(out: &mut Vec<Benchmark>) {
    apply::register::<D>(out);
    build::register::<D>(out);
    edit_trace::register::<D>(out);
    egwalker_paper::register::<D>(out);
    length::register::<D>(out);
    list::register::<D>(out);
    load_save::register::<D>(out);
    map::register::<D>(out);
    marks::register::<D>(out);
    memory::register::<D>(out);
    range::register::<D>(out);
    sync::register::<D>(out);
    typing::register::<D>(out);
    not_ported::register::<D>(out);
}
