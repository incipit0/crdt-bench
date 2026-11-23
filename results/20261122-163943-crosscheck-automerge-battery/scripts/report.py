"""Cross-check report: Automerge's rust/benchmark-battery vs crdt-bench's port.

usage: report.py <crosscheck dir> <crdt-bench 0.12.0 battery.json>

Reads (all Automerge results):
  automerge-battery/battery.json             their battery, dev Automerge, their profile (fat LTO, 1 CGU)
  crdt-bench-dev-automerge/battery.json      our port, same dev Automerge, crdt-bench's default profile
  crdt-bench-dev-automerge-lto/battery.json  our port, same dev Automerge, their profile
  <0.12.0 battery.json>                      our port, crates.io automerge 0.12.0, default profile
Writes comparison.md, comparison.csv and harness-ratio-by-group.png into <crosscheck dir>.
"""
import csv
import json
import math
import statistics
import sys
from collections import defaultdict
from pathlib import Path

out = Path(sys.argv[1])
load = lambda p: json.load(open(p))["results"]
RENAME = {"load_actors/": "load_save/load_actors/"}


def our_base(name):
    for a, b in RENAME.items():
        if name.startswith(a):
            return b + name[len(a):]
    return name


theirs = {our_base(r["name"]): r for r in load(out / "automerge-battery/battery.json")}
ours_of = lambda p: {r["base"]: r for r in load(p) if r.get("library") == "automerge"}
ours = ours_of(out / "crdt-bench-dev-automerge/battery.json")
lto = ours_of(out / "crdt-bench-dev-automerge-lto/battery.json")
v12 = ours_of(sys.argv[2])


def metrics(r):
    if not r:
        return {}
    k = r["kind"]
    if k == "sampled":
        return {"median": (r["median"], r["min"], r["max"])}
    if k == "series":
        return {"total": (r["summary"]["total"], None, None)}
    if k == "memory":
        return {"peak": (r["peak_bytes"], None, None), "steady": (r["steady_bytes"], None, None)}
    return {}


rows = []
for base in sorted(theirs):
    tm, om = metrics(theirs[base]), metrics(ours.get(base))
    if not tm or not om:
        continue
    lm, vm = metrics(lto.get(base)), metrics(v12.get(base))
    for m, (tv, tlo, thi) in tm.items():
        ov, olo, ohi = om[m]
        lv = lm.get(m, (None,) * 3)
        rows.append(dict(
            base=base, group=base.split("/")[0], kind=theirs[base]["kind"], metric=m,
            theirs=tv, ours=ov, lto=lv[0], v012=vm.get(m, (None,))[0],
            r_default=ov / tv, r_lto=lv[0] / tv if lv[0] else None,
            overlap_lto=None if tlo is None or lv[1] is None else not (lv[2] < tlo or thi < lv[1]),
            version=ov / vm[m][0] if m in vm and vm[m][0] else None))

theirs_only = [b for b in sorted(theirs) if metrics(theirs[b]) and not metrics(ours.get(b))]
ours_only = [b for b in sorted(ours) if metrics(ours[b]) and b not in theirs]
gm = lambda xs: math.exp(statistics.mean(math.log(x) for x in xs)) if xs else float("nan")
timed = [r for r in rows if r["kind"] != "memory"]
mem = [r for r in rows if r["kind"] == "memory"]


def band(rs, key, lo, hi):
    return sum(1 for r in rs if r[key] and lo <= r[key] <= hi)


def fmt(v, unit):
    if v is None:
        return "–"
    if unit == "bytes":
        return f"{v / 2**20:.2f} MiB"
    for s, d in (("s", 1e9), ("ms", 1e6), ("µs", 1e3)):
        if v >= d:
            return f"{v / d:.2f} {s}"
    return f"{v} ns"


# ----- CSV -----
with open(out / "comparison.csv", "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
    w.writeheader()
    w.writerows(rows)

# ----- chart: ours/theirs per group, default profile vs LTO -----
try:
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    groups = sorted({r["group"] for r in timed}, key=lambda g: -gm([r["r_default"] for r in timed if r["group"] == g]))
    fig, ax = plt.subplots(figsize=(10, 0.42 * len(groups) + 1.6))
    for i, g in enumerate(groups):
        rs = [r for r in timed if r["group"] == g]
        for key, color, dy, label in (("r_default", "#eb6834", 0.13, "crdt-bench default profile"),
                                      ("r_lto", "#2a78d6", -0.13, "crdt-bench with Automerge's profile (LTO)")):
            xs = [r[key] for r in rs if r[key]]
            ax.scatter(xs, [i + dy] * len(xs), s=22, color=color, alpha=0.55, linewidths=0,
                       label=label if i == 0 else None)
            ax.scatter([gm(xs)], [i + dy], s=90, marker="|", color=color, linewidths=2.5)
    ax.axvline(1, color="#262624", lw=1)
    for x in (0.8, 1.25):
        ax.axvline(x, color="#c9c8c1", lw=1, ls="--")
    ax.set_xscale("log")
    ax.set_xticks([0.6, 0.7, 0.8, 0.9, 1, 1.1, 1.25, 1.5], ["0.6", "0.7", "0.8", "0.9", "1", "1.1", "1.25", "1.5"])
    ax.minorticks_off()
    ax.set_yticks(range(len(groups)), groups)
    ax.invert_yaxis()
    ax.set_xlabel("crdt-bench ÷ Automerge battery (same dev Automerge)  ·  dots: benchmarks, bars: group geomean  ·  dashed: ±25%")
    for s in ("top", "right"):
        ax.spines[s].set_visible(False)
    ax.legend(loc="lower right", bbox_to_anchor=(1, 1.0), ncol=2, frameon=False, fontsize=9)
    fig.suptitle("Our port vs the original battery: the default build profile explains the slowdown",
                 x=0.01, ha="left", fontsize=13, fontweight="bold")
    fig.tight_layout()
    fig.savefig(out / "harness-ratio-by-group.png", dpi=150, facecolor="white")
    chart = True
except ImportError:
    chart = False

# ----- Markdown -----
L = []
p = L.append
p("# Cross-check: Automerge's benchmark battery vs crdt-bench's port\n")
p("Both batteries were run on the same machine, one after the other, against the **same development Automerge** "
  "(the automerge checkout at `09b19d508`, *rust/automerge: prototype bounded fragment boundary memoization*), "
  "all tiers, 10 samples, 1 warmup, rustc 1.96.1. Only the `automerge` results of crdt-bench are compared.\n")
p("| Run | Battery | Automerge | Build profile |\n|---|---|---|---|")
p("| theirs | `rust/benchmark-battery` | dev `09b19d508` | fat LTO, 1 codegen unit (their workspace) |")
p("| ours default | crdt-bench port | dev `09b19d508` | Cargo default (no fat LTO, 16 codegen units) |")
p("| ours LTO | crdt-bench port | dev `09b19d508` | fat LTO, 1 codegen unit (copied from theirs) |")
p("| ours 0.12.0 | crdt-bench port | crates.io 0.12.0 | Cargo default (`20261119-170021-battery-all-tiers`) |\n")
p("## Summary\n")
p(f"- **{len(timed)} timing metrics and {len(mem)} memory metrics** are measured by both batteries "
  f"({len(theirs_only)} benchmarks only theirs, {len(ours_only)} only ours).")
p(f"- **Memory agrees exactly:** geomean ratio {gm([r['r_default'] for r in mem]):.3f} over {len(mem)} metrics.")
p(f"- **Timing with crdt-bench's default profile:** geomean {gm([r['r_default'] for r in timed]):.3f}; "
  f"within ±10%: {band(timed, 'r_default', 1/1.1, 1.1)}/{len(timed)}, within ±25%: {band(timed, 'r_default', 0.8, 1.25)}/{len(timed)}.")
lt = [r for r in timed if r["r_lto"]]
p(f"- **Timing with Automerge's profile (fat LTO, 1 codegen unit):** geomean {gm([r['r_lto'] for r in lt]):.3f}; "
  f"within ±10%: {band(lt, 'r_lto', 1/1.1, 1.1)}/{len(lt)}, within ±25%: {band(lt, 'r_lto', 0.8, 1.25)}/{len(lt)}.")
p("- The port's workloads match the original. Most of the earlier gap came from the build profile, not from the benchmarks: "
  "crdt-bench's release build doesn't use fat LTO, which costs most where a benchmark makes many small calls into Automerge.\n")
if chart:
    p("![crdt-bench ÷ Automerge battery per group, default profile vs LTO](harness-ratio-by-group.png)\n")

p("## Remaining differences (LTO build, beyond ±15%)\n")
rem = sorted([r for r in lt if not (1 / 1.15 <= r["r_lto"] <= 1.15)], key=lambda r: -abs(math.log(r["r_lto"])))
if rem:
    p("| Benchmark | Theirs | Ours (LTO) | Ratio | Ranges overlap |\n|---|---:|---:|---:|---|")
    for r in rem:
        unit = theirs[r["base"]]["unit"]
        ov = {True: "yes", False: "no", None: "–"}[r["overlap_lto"]]
        p(f"| `{r['base']}` ({r['metric']}) | {fmt(r['theirs'], unit)} | {fmt(r['lto'], unit)} | {r['r_lto']:.2f} | {ov} |")
else:
    p("None.")
p("")
p("Why they remain:\n")
p("- **`marks/get_marks_at_end`**: data-dependent. The original battery places its 10 marks with an unseeded RNG, "
  "the port with a seeded one, so the two documents have different marks on their last character.")
p("- **`load_save/load_actors/load/{100,1000}`**: different documents. The original gives every writer an Automerge "
  "*author* (`with_author`) and a random 16-byte actor ID; the port's cross-library API has no authors and uses 8-byte "
  "actor IDs. Loading the author records is extra work, so the original is slower.")
p("- **`sync/tiny_text`**: the port encodes every sync message to bytes and decodes it again (as on a network, the same "
  "for every library); the original passes `Message` values directly.")
p("- **`length/map_len_now`**: same keys and values, but the original builds the map with the low-level `Automerge` "
  "API and the port with `AutoCommit`. Not investigated; both are under 2 ms.")
p("- **`edit_trace/edit_trace_single_tx`, `map/save/repeated_put/10000`, `list/list_cursor_now`**: 16–18%, the last "
  "within its min–max range. Not investigated.\n")
p("## Automerge dev vs 0.12.0 (our harness, default profile)\n")
p("Same harness and machine, only the Automerge version differs. Largest changes:\n")
p("| Benchmark | 0.12.0 | dev | dev ÷ 0.12.0 |\n|---|---:|---:|---:|")
for r in sorted([r for r in timed if r["version"]], key=lambda r: -abs(math.log(r["version"])))[:12]:
    unit = theirs[r["base"]]["unit"]
    p(f"| `{r['base']}` ({r['metric']}) | {fmt(r['v012'], unit)} | {fmt(r['ours'], unit)} | {r['version']:.3g} |")
p("")
p("## All shared metrics\n")
p("| Benchmark | Metric | Theirs | Ours default | ÷ | Ours LTO | ÷ |\n|---|---|---:|---:|---:|---:|---:|")
for r in rows:
    unit = theirs[r["base"]]["unit"]
    rl = f"{r['r_lto']:.2f}" if r["r_lto"] else "–"
    p(f"| `{r['base']}` | {r['metric']} | {fmt(r['theirs'], unit)} | {fmt(r['ours'], unit)} | {r['r_default']:.2f} "
      f"| {fmt(r['lto'], unit)} | {rl} |")
p("")
p(f"## Only in one battery\n\n**Only theirs** ({len(theirs_only)}): Automerge-only features (ticket 2) and real-world "
  "fixtures (ticket 3), which crdt-bench registers as not run:\n")
p(", ".join(f"`{b}`" for b in theirs_only) + "\n")
p(f"**Only ours** ({len(ours_only)}): the S2 trace, which the original battery leaves out: "
  + ", ".join(f"`{b}`" for b in ours_only) + "\n")
p("## Files\n")
p("- `automerge-battery/`: their results (`battery.json`) and Markdown report")
p("- `crdt-bench-dev-automerge/`, `crdt-bench-dev-automerge-lto/`: our experiments (`battery.json`, `meta.json`, `report.md`)")
p("- `comparison.csv`: every shared metric with both ratios; `harness-ratio-by-group.png`: the chart above")
(out / "comparison.md").write_text("\n".join(L) + "\n")
print(f"wrote comparison.md ({len(rows)} rows), comparison.csv" + (", harness-ratio-by-group.png" if chart else ""))
