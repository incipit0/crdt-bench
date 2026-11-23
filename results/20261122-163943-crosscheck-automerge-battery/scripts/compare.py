"""Compare Automerge's rust/benchmark-battery with crdt-bench's port.

usage: compare.py <crosscheck dir> <crdt-bench 0.12.0 battery.json>

<crosscheck dir>/automerge-battery/battery.json        their battery, dev Automerge
<crosscheck dir>/crdt-bench-dev-automerge/battery.json  our battery, same dev Automerge
Writes <crosscheck dir>/comparison.json (rows) for the report and charts.
"""
import json
import sys
from pathlib import Path

out = Path(sys.argv[1])
theirs_run = json.load(open(out / "automerge-battery" / "battery.json"))
ours_run = json.load(open(out / "crdt-bench-dev-automerge" / "battery.json"))
ours12_run = json.load(open(sys.argv[2]))

RENAME = {"load_actors/": "load_save/load_actors/"}  # their name prefix -> ours


def our_base(name: str) -> str:
    for a, b in RENAME.items():
        if name.startswith(a):
            return b + name[len(a):]
    return name


def index_theirs(run):
    return {our_base(r["name"]): r for r in run["results"]}


def index_ours(run):
    return {r["base"]: r for r in run["results"] if r.get("library") == "automerge"}


def metrics(r):
    """(metric name, value, lo, hi) tuples for a measured result."""
    if r["kind"] == "sampled":
        return [("median", r["median"], r["min"], r["max"])]
    if r["kind"] == "series":
        s = r["summary"]
        return [("total", s["total"], None, None), ("step median", s["median"], None, None)]
    if r["kind"] == "memory":
        return [("peak", r["peak_bytes"], None, None), ("steady", r["steady_bytes"], None, None)]
    return []


theirs, ours, ours12 = index_theirs(theirs_run), index_ours(ours_run), index_ours(ours12_run)
rows, theirs_only, ours_only = [], [], []
for base in sorted(set(theirs) | set(ours)):
    t, o, o12 = theirs.get(base), ours.get(base), ours12.get(base)
    tm = metrics(t) if t else []
    om = metrics(o) if o else []
    if tm and om:
        o12m = {m[0]: m for m in metrics(o12)} if o12 else {}
        for (name, tv, tlo, thi), (_, ov, olo, ohi) in zip(tm, om):
            v12 = o12m.get(name, (None, None))[1]
            overlap = None if tlo is None else not (ohi < tlo or thi < olo)
            rows.append(dict(base=base, group=base.split("/")[0], kind=t["kind"], metric=name, unit=t["unit"],
                             theirs=tv, ours=ov, ours12=v12, ratio=ov / tv if tv else None,
                             version=ov / v12 if v12 else None, overlap=overlap))
    elif tm:
        theirs_only.append(dict(base=base, kind=t["kind"], metrics=tm, unit=t["unit"],
                                ours_reason=(o or {}).get("reason")))
    elif om:
        ours_only.append(dict(base=base, kind=o["kind"], metrics=om, unit=o["unit"]))

meta = dict(theirs_commit=theirs_run.get("commit"), theirs_rustc=theirs_run.get("rustc"),
            ours_rustc=ours_run.get("rustc"), sample_count=theirs_run.get("sample_count"))
json.dump(dict(meta=meta, rows=rows, theirs_only=theirs_only, ours_only=ours_only),
          open(out / "comparison.json", "w"), indent=1)

within = lambda lo, hi: sum(1 for r in rows if r["ratio"] and lo <= r["ratio"] <= hi)
print(f"{len(rows)} matched metrics; within ±10%: {within(1/1.1, 1.1)}; within ±25%: {within(0.8, 1.25)}")
print(f"theirs only: {len(theirs_only)}; ours only: {len(ours_only)}")
for r in sorted(rows, key=lambda r: -abs(__import__('math').log(r['ratio'] or 1)))[:15]:
    print(f"  {r['ratio']:6.2f}x  {r['base']} [{r['metric']}]  overlap={r['overlap']}")
