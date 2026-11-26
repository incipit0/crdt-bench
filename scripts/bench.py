#!/usr/bin/env python3
"""Run CRDT benchmarks and turn the results into graphs.

    python scripts/bench.py run seq_add --sizes 6000,60000,600000
    python scripts/bench.py run load mem --file-sizes 6k,60k,600k --label am3
    python scripts/bench.py plot results/latest            # re-plot a run
    python scripts/bench.py plot results/am2 results/am3   # compare runs
    python scripts/bench.py battery                        # plot the latest battery experiment and
                                                           # add the charts to its report.md
    python scripts/bench.py battery results/<experiment>   # ...or a given one (plots/ inside it)

Each `run` writes to results/<timestamp>[-<label>]/:
    criterion/    raw Criterion output (CRITERION_HOME for this run only)
    mem.jsonl     heap stats from the `mem` workload (one JSON object per line)
    meta.json     git commit, crate versions, command line
    plots/        one PNG (and optionally SVG) per benchmark group, index.html,
                  summary.csv (every estimate) and memory.csv
"""

from __future__ import annotations

import argparse
import csv
import datetime as dt
import html
import json
import os
import re
import subprocess
import sys
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RESULTS = ROOT / "results"

CRITERION_WORKLOADS = ["seq_add", "conc_add", "load", "version", "real"]
WORKLOADS = CRITERION_WORKLOADS + ["mem"]
MEM_LIBS = ["automerge", "yrs"]
DEFAULT_MEM_FILE_SIZES = ["6k", "60k", "600k"]

# What the swept parameter means, per group-name prefix. Falls back to "parameter".
X_LABELS = {
    "Insert characters": "characters inserted",
    "Apply chars": "characters inserted",
    "Append one big text": "text length (chars)",
    "Apply one big change": "text length (chars)",
    "Time loading doc from memory": "document length (chars)",
    "Time loading doc from disk": "file (document length)",
}

# Categorical palette (light mode), fixed order. Colour follows the library,
# never its rank: Automerge is always slot 1, Yjs/Yrs always slot 2,
# Automerge hashless (hashless branch) always slot 3.
PALETTE = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"]
PREFERRED_SLOT = {"Automerge": 0, "Yjs": 1, "Yrs": 1, "Automerge hashless": 2}

# Secondary encoding for the run when comparing runs, so identity is not colour-only.
LINESTYLES = ["-", "--", ":", "-."]
HATCHES = [None, "///", "...", "xxx"]
RUN_INDEX: dict[str, int] = {}  # run label -> index into LINESTYLES / HATCHES, set by plot_runs

INK = "#0b0b0b"
INK_2 = "#52514e"
GRID = "#e4e3df"
SURFACE = "#fcfcfb"


# --------------------------------------------------------------------------- running


def sh(cmd: list[str], **kw) -> str:
    try:
        return subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, check=True, **kw).stdout.strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        return ""


def crate_versions() -> dict[str, str]:
    lock = (ROOT / "Cargo.lock").read_text()
    out = {}
    for name in ("automerge", "yrs", "criterion"):
        m = re.search(rf'name = "{name}"\nversion = "([^"]+)"', lock)
        if m:
            out[name] = m.group(1)
    return out


def write_meta(run_dir: Path, args: argparse.Namespace) -> None:
    meta = {
        "label": args.label,
        "started": dt.datetime.now().isoformat(timespec="seconds"),
        "workloads": args.workloads,
        "argv": sys.argv,
        "git_commit": sh(["git", "rev-parse", "--short", "HEAD"]),
        "git_dirty": bool(sh(["git", "status", "--porcelain", "--untracked-files=no"])),
        "rustc": sh(["rustc", "--version"]),
        "crates": crate_versions(),
        "env": {k: v for k, v in bench_env(args).items() if k.startswith("CRDT_")},
    }
    (run_dir / "meta.json").write_text(json.dumps(meta, indent=2))


def bench_env(args: argparse.Namespace) -> dict[str, str]:
    env = {}
    if args.sizes:
        env["CRDT_SIZES"] = args.sizes
    if args.file_sizes:
        env["CRDT_FILE_SIZES"] = args.file_sizes
    if args.sample_size:
        env["CRDT_SAMPLE_SIZE"] = str(args.sample_size)
    return env


def run_criterion(workload: str, run_dir: Path, args: argparse.Namespace) -> None:
    env = os.environ | bench_env(args) | {"CRITERION_HOME": str(run_dir / "criterion")}
    cmd = ["cargo", "bench", "--bench", workload, "--"]
    if args.filter:
        cmd.append(args.filter)
    if args.quick:
        cmd.append("--quick")
    if not args.criterion_html:
        cmd.append("--noplot")
    print(f"\n==> {' '.join(cmd)}", flush=True)
    subprocess.run(cmd, cwd=ROOT, env=env, check=True)


def run_mem(run_dir: Path, args: argparse.Namespace) -> None:
    out = run_dir / "mem.jsonl"
    sizes = (args.file_sizes or ",".join(DEFAULT_MEM_FILE_SIZES)).split(",")
    for size in sizes:
        for lib in MEM_LIBS:
            env = os.environ | {"CRDT_LIB": lib, "CRDT_FILE_SIZE": size.strip(), "CRDT_MEM_OUT": str(out)}
            cmd = ["cargo", "bench", "--bench", "mem", "--features", "dhat-heap"]
            print(f"\n==> CRDT_LIB={lib} CRDT_FILE_SIZE={size} {' '.join(cmd)}", flush=True)
            # dhat drops dhat-heap.json in the cwd; keep it inside the run dir.
            subprocess.run(cmd, cwd=ROOT, env=env, check=True)
            heap = ROOT / "dhat-heap.json"
            if heap.exists():
                heap.replace(run_dir / f"dhat-{lib}-{size}.json")


def cmd_run(args: argparse.Namespace) -> None:
    workloads = WORKLOADS if "all" in args.workloads else args.workloads
    for w in workloads:
        if w not in WORKLOADS:
            sys.exit(f"unknown workload {w!r}; choose from {', '.join(WORKLOADS)} or all")
    args.workloads = workloads

    stamp = dt.datetime.now().strftime("%Y%m%d-%H%M%S")
    run_dir = RESULTS / (f"{stamp}-{args.label}" if args.label else stamp)
    run_dir.mkdir(parents=True)
    write_meta(run_dir, args)
    print(f"results -> {run_dir.relative_to(ROOT)}")

    for w in workloads:
        if w == "mem":
            run_mem(run_dir, args)
        else:
            run_criterion(w, run_dir, args)

    latest = RESULTS / "latest"
    if latest.is_symlink() or latest.exists():
        latest.unlink()
    latest.symlink_to(run_dir.name)

    plot_runs([run_dir], run_dir / "plots", args.formats)


# --------------------------------------------------------------------------- loading


@dataclass
class Estimate:
    run: str  # run label (used to tell runs apart when comparing)
    group: str
    series: str  # library / function id
    param: str | None  # swept parameter as Criterion recorded it
    mean_ns: float
    lo_ns: float
    hi_ns: float
    median_ns: float
    n_samples: int


def run_label(run_dir: Path) -> str:
    meta = run_dir / "meta.json"
    if meta.exists():
        label = json.loads(meta.read_text()).get("label")
        if label:
            return label
    return run_dir.resolve().name


def load_criterion(run_dir: Path) -> list[Estimate]:
    base = run_dir / "criterion" if (run_dir / "criterion").is_dir() else run_dir
    label = run_label(run_dir)
    out = []
    for bj in sorted(base.rglob("new/benchmark.json")):
        meta = json.loads(bj.read_text())
        est = json.loads((bj.parent / "estimates.json").read_text())
        sample_file = bj.parent / "sample.json"
        n = len(json.loads(sample_file.read_text())["iters"]) if sample_file.exists() else 0
        series, param = meta.get("function_id"), meta.get("value_str")
        if series is None:  # `bench_with_input(BenchmarkId::from_parameter(..))`
            series, param = param or meta["group_id"], None
        out.append(
            Estimate(
                run=label,
                group=meta["group_id"],
                series=series,
                param=param,
                mean_ns=est["mean"]["point_estimate"],
                lo_ns=est["mean"]["confidence_interval"]["lower_bound"],
                hi_ns=est["mean"]["confidence_interval"]["upper_bound"],
                median_ns=est["median"]["point_estimate"],
                n_samples=n,
            )
        )
    return out


def load_mem(run_dir: Path) -> list[dict]:
    f = run_dir / "mem.jsonl"
    if not f.exists():
        return []
    rows = [json.loads(line) for line in f.read_text().splitlines() if line.strip()]
    label = run_label(run_dir)
    for r in rows:
        r["run"] = label
    return rows


# --------------------------------------------------------------------------- plotting

SUFFIX = {"k": 1e3, "K": 1e3, "m": 1e6, "M": 1e6, "g": 1e9, "G": 1e9}


def numeric(p: str | None) -> float | None:
    """'6000' -> 6000, '60k' -> 60000, '1.5M' -> 1.5e6, anything else -> None."""
    if p is None:
        return None
    m = re.fullmatch(r"\s*([0-9]*\.?[0-9]+)\s*([kKmMgG]?)\s*", p)
    if not m:
        return None
    return float(m.group(1)) * SUFFIX.get(m.group(2), 1)


def time_unit(max_ns: float) -> tuple[str, float]:
    for unit, scale in (("s", 1e9), ("ms", 1e6), ("µs", 1e3)):
        if max_ns >= scale:
            return unit, scale
    return "ns", 1.0


def byte_unit(max_b: float) -> tuple[str, float]:
    for unit, scale in (("GiB", 2**30), ("MiB", 2**20), ("KiB", 2**10)):
        if max_b >= scale:
            return unit, scale
    return "B", 1.0


def fmt(v: float) -> str:
    return f"{v:.3g}" if v < 1000 else f"{v:,.0f}"


def slug(s: str) -> str:
    return re.sub(r"[^A-Za-z0-9]+", "-", s).strip("-").lower()[:80]


class Colors:
    """Assigns palette slots once per plotting session so a series keeps its colour in every chart."""

    def __init__(self, names: list[str]):
        self.map: dict[str, str] = {}
        taken = set()
        for name in names:
            base = name.split(" (")[0]  # "Automerge (am3)" -> "Automerge"
            slot = PREFERRED_SLOT.get(base)
            if slot is not None and slot not in taken and base == name:
                self.map[name] = PALETTE[slot]
                taken.add(slot)
        free = [i for i in range(len(PALETTE)) if i not in taken]
        for name in names:
            if name not in self.map:
                if not free:
                    raise SystemExit(f"more than {len(PALETTE)} series; filter the benchmarks or compare fewer runs")
                self.map[name] = PALETTE[free.pop(0)]

    def __getitem__(self, name: str) -> str:
        return self.map[name]


def setup_mpl():
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    plt.rcParams.update(
        {
            "figure.facecolor": SURFACE,
            "axes.facecolor": SURFACE,
            "savefig.facecolor": SURFACE,
            "font.size": 10,
            "axes.edgecolor": GRID,
            "axes.labelcolor": INK_2,
            "axes.titlecolor": INK,
            "axes.titlesize": 12,
            "axes.titleweight": "bold",
            "axes.titlelocation": "left",
            "axes.spines.top": False,
            "axes.spines.right": False,
            "axes.grid": True,
            "axes.axisbelow": True,
            "grid.color": GRID,
            "grid.linewidth": 0.8,
            "xtick.color": INK_2,
            "ytick.color": INK_2,
            "xtick.major.size": 0,
            "ytick.major.size": 0,
            "legend.frameon": False,
            "legend.labelcolor": INK,
        }
    )
    return plt


def series_name(e, multi_run: bool) -> str:
    s = e.series if isinstance(e, Estimate) else e["lib"]
    run = e.run if isinstance(e, Estimate) else e["run"]
    return f"{s} ({run})" if multi_run else s


def footer(fig, metas: list[dict]) -> None:
    parts = []
    for m in metas:
        crates = " · ".join(f"{k} {v}" for k, v in m.get("crates", {}).items() if k != "criterion")
        dirty = "+dirty" if m.get("git_dirty") else ""
        tag = f"{m['label']}: " if len(metas) > 1 and m.get("label") else ""
        parts.append(f"{tag}{crates} · {m.get('git_commit', '?')}{dirty} · {m.get('started', '')[:10]}")
    fig.text(0.01, 0.01, "   |   ".join(parts), fontsize=7.5, color=INK_2, ha="left", va="bottom")


def plot_group(plt, group: str, rows: list[Estimate], colors: Colors, multi_run: bool, metas, out: Path, formats):
    by_series: dict[str, list[Estimate]] = defaultdict(list)
    for r in rows:
        by_series[series_name(r, multi_run)].append(r)
    params = sorted({r.param for r in rows if r.param is not None}, key=lambda p: (numeric(p) is None, numeric(p) or 0, p))
    sweep = len(params) >= 2 and all(numeric(p) is not None for p in params)
    unit, scale = time_unit(max(r.hi_ns for r in rows))
    xlabel = next((v for k, v in X_LABELS.items() if group.startswith(k)), "parameter")

    fig, ax = plt.subplots(figsize=(7.5, 4.5))
    if sweep:
        xs_all = [numeric(p) for p in params]
        logx = max(xs_all) / min(xs_all) >= 10
        ys_all = [r.mean_ns for r in rows]
        logy = max(ys_all) / max(min(ys_all), 1e-9) >= 30
        for name, rs in by_series.items():
            rs = sorted(rs, key=lambda r: numeric(r.param))
            xs = [numeric(r.param) for r in rs]
            ys = [r.mean_ns / scale for r in rs]
            c = colors[name]
            ax.fill_between(xs, [r.lo_ns / scale for r in rs], [r.hi_ns / scale for r in rs], color=c, alpha=0.15, lw=0)
            ls = LINESTYLES[RUN_INDEX.get(rs[0].run, 0) % len(LINESTYLES)]
            ax.plot(xs, ys, color=c, lw=2, ls=ls, marker="o", ms=6, mec=SURFACE, mew=1.5, label=name)
            # direct label at the end of each line (only while it stays readable)
            if len(by_series) <= 4:
                ax.annotate(name, (xs[-1], ys[-1]), xytext=(6, 0), textcoords="offset points", va="center", fontsize=9, color=INK)
        if logx:
            ax.set_xscale("log")
        if logy:
            ax.set_yscale("log")
        ax.set_xticks(xs_all, params)
        ax.minorticks_off()
        ax.set_xlabel(xlabel)
        ax.margins(x=0.12)
    else:
        # grouped bars: x = parameter (or a single slot), one bar per series
        cats = params or [None]
        names = list(by_series)
        width = 0.8 / len(names)
        nbars = len(cats) * len(names)
        for i, name in enumerate(names):
            lookup = {r.param: r for r in by_series[name]}
            xs, ys, err = [], [], [[], []]
            for j, cat in enumerate(cats):
                r = lookup.get(cat)
                if r is None:
                    continue
                xs.append(j - 0.4 + width * (i + 0.5))
                ys.append(r.mean_ns / scale)
                err[0].append((r.mean_ns - r.lo_ns) / scale)
                err[1].append((r.hi_ns - r.mean_ns) / scale)
            hatch = HATCHES[RUN_INDEX.get(by_series[name][0].run, 0) % len(HATCHES)]
            bars = ax.bar(xs, ys, width * 0.92, color=colors[name], hatch=hatch, edgecolor=SURFACE, lw=0, label=name,
                          yerr=err, ecolor=INK_2, capsize=0, error_kw={"lw": 1})
            if nbars <= 8:
                ax.bar_label(bars, labels=[fmt(y) for y in ys], padding=3, fontsize=9, color=INK)
        ax.set_xticks(range(len(cats)), [c if c is not None else "" for c in cats])
        if params:
            ax.set_xlabel(xlabel)
        ax.grid(axis="x", visible=False)
        ax.margins(y=0.12)
    ax.set_ylabel(f"time per iteration ({unit})")
    ax.set_title(group, pad=20)
    n = max(r.n_samples for r in rows)
    ax.text(0, 1.01, f"mean with 95% CI · {n} samples", transform=ax.transAxes, fontsize=8.5, color=INK_2, va="bottom")
    if len(by_series) > 1:
        ax.legend(loc="upper left", bbox_to_anchor=(0.0, -0.14), ncol=min(len(by_series), 4), fontsize=9, handlelength=2.5)
    plain_log_labels(ax)
    footer(fig, metas)
    fig.tight_layout(rect=(0, 0.04, 1, 0.97))
    paths = save(fig, out, slug(group), formats)
    plt.close(fig)
    return paths


def plot_mem(plt, rows: list[dict], colors: Colors, multi_run: bool, metas, out: Path, formats):
    sizes = sorted({r["file_size"] for r in rows}, key=lambda s: numeric(s) or 0)
    names = list(dict.fromkeys(series_name(r, multi_run) for r in rows))
    unit, scale = byte_unit(max(r["max_bytes"] for r in rows))
    fig, axes = plt.subplots(1, 2, figsize=(10, 4.2), sharey=True)
    for ax, key, title in ((axes[0], "max_bytes", "Peak heap while loading"), (axes[1], "curr_bytes", "Heap retained after load")):
        width = 0.8 / len(names)
        for i, name in enumerate(names):
            lookup = {r["file_size"]: r for r in rows if series_name(r, multi_run) == name}
            xs = [j - 0.4 + width * (i + 0.5) for j, s in enumerate(sizes) if s in lookup]
            ys = [lookup[s][key] / scale for s in sizes if s in lookup]
            hatch = HATCHES[RUN_INDEX.get(next(r["run"] for r in rows if series_name(r, multi_run) == name), 0) % len(HATCHES)]
            bars = ax.bar(xs, ys, width * 0.92, color=colors[name], hatch=hatch, edgecolor=SURFACE, lw=0, label=name)
            if len(sizes) * len(names) <= 8:
                ax.bar_label(bars, labels=[fmt(y) for y in ys], padding=3, fontsize=9, color=INK)
        ax.set_xticks(range(len(sizes)), sizes)
        ax.set_xlabel("document file")
        ax.set_title(title, fontsize=11)
        ax.grid(axis="x", visible=False)
        ax.margins(y=0.12)
    if max(r["max_bytes"] for r in rows) / max(min(r["curr_bytes"] for r in rows), 1) >= 100:
        axes[0].set_yscale("log")
    axes[0].set_ylabel(f"heap ({unit}, dhat)")
    handles, labels = axes[0].get_legend_handles_labels()
    fig.legend(handles, labels, loc="lower left", bbox_to_anchor=(0.06, 0.05), ncol=min(len(names), 4), fontsize=9)
    for ax in axes:
        ax.minorticks_off()
        plain_log_labels(ax)
    fig.suptitle("Memory when loading a document from disk", x=0.01, ha="left", fontweight="bold", color=INK)
    footer(fig, metas)
    fig.tight_layout(rect=(0, 0.1, 1, 0.97))
    paths = save(fig, out, "memory", formats)
    plt.close(fig)
    return paths


def plain_log_labels(ax) -> None:
    """Log axes read better as 0.1, 1, 10, 100 than as 10^-1, 10^0, ..."""
    from matplotlib.ticker import FuncFormatter

    if ax.get_yscale() == "log":
        ax.yaxis.set_major_formatter(FuncFormatter(lambda v, _: f"{v:g}"))


def save(fig, out: Path, name: str, formats: list[str]) -> list[Path]:
    paths = []
    for f in formats:
        p = out / f"{name}.{f}"
        fig.savefig(p, dpi=160 if f == "png" else None)
        paths.append(p)
    return paths


def plot_runs(run_dirs: list[Path], out: Path, formats: list[str]) -> None:
    plt = setup_mpl()
    out.mkdir(parents=True, exist_ok=True)
    multi_run = len(run_dirs) > 1
    if multi_run:
        labels = [run_label(d) for d in run_dirs]
        if len(set(labels)) != len(labels):
            sys.exit(f"runs need distinct labels to be compared, got {labels}")

    estimates = [e for d in run_dirs for e in load_criterion(d)]
    mem = [r for d in run_dirs for r in load_mem(d)]
    metas = [json.loads((d / "meta.json").read_text()) if (d / "meta.json").exists() else {} for d in run_dirs]
    if not estimates and not mem:
        sys.exit(f"no results found in {', '.join(map(str, run_dirs))}")

    all_names = list(dict.fromkeys([series_name(e, multi_run) for e in estimates] + [series_name(r, multi_run) for r in mem]))
    colors = Colors(all_names)  # first-appearance order: run 1's libraries, then run 2's, ...
    RUN_INDEX.clear()
    RUN_INDEX.update({run_label(d): i for i, d in enumerate(run_dirs)})

    groups: dict[str, list[Estimate]] = defaultdict(list)
    for e in estimates:
        groups[e.group].append(e)

    figures: list[tuple[str, Path]] = []
    for group, rows in groups.items():
        paths = plot_group(plt, group, rows, colors, multi_run, metas, out, formats)
        figures.append((group, paths[0]))
    if mem:
        figures.append(("Memory", plot_mem(plt, mem, colors, multi_run, metas, out, formats)[0]))

    write_summary(out, estimates, mem)
    write_index(out, figures, run_dirs)
    print(f"\n{len(figures)} chart(s) -> {out}")
    for _, p in figures:
        print(f"  {p}")


def write_summary(dest: Path, estimates: list[Estimate], mem: list[dict]) -> None:
    with open(dest / "summary.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["run", "group", "series", "param", "mean_ns", "ci_lo_ns", "ci_hi_ns", "median_ns", "samples"])
        for e in estimates:
            w.writerow([e.run, e.group, e.series, e.param or "", f"{e.mean_ns:.1f}", f"{e.lo_ns:.1f}", f"{e.hi_ns:.1f}", f"{e.median_ns:.1f}", e.n_samples])
    if mem:
        with open(dest / "memory.csv", "w", newline="") as f:
            keys = ["run", "lib", "file_size", "max_bytes", "curr_bytes", "max_blocks", "curr_blocks", "total_bytes"]
            w = csv.DictWriter(f, fieldnames=keys, extrasaction="ignore")
            w.writeheader()
            w.writerows(mem)


def write_index(out: Path, figures: list[tuple[str, Path]], run_dirs: list[Path]) -> None:
    items = "\n".join(
        f'<figure><img src="{html.escape(p.name)}" alt="{html.escape(t)}"><figcaption>{html.escape(t)}</figcaption></figure>' for t, p in figures
    )
    runs = ", ".join(html.escape(str(d)) for d in run_dirs)
    (out / "index.html").write_text(
        f"""<!doctype html><meta charset="utf-8"><title>CRDT bench results</title>
<style>body{{font:14px system-ui,sans-serif;margin:24px;background:#fcfcfb;color:#0b0b0b}}
figure{{margin:0 0 32px}}img{{max-width:100%;height:auto}}figcaption{{color:#52514e}}</style>
<h1>CRDT bench results</h1><p>{runs}</p>
{items}
"""
    )


def cmd_plot(args: argparse.Namespace) -> None:
    run_dirs = [Path(d) for d in args.runs]
    for d in run_dirs:
        if not d.is_dir():
            sys.exit(f"{d} is not a directory")
    if args.out:
        out = Path(args.out)
    elif len(run_dirs) == 1:
        out = run_dirs[0] / "plots"
    else:
        out = RESULTS / "compare" / "-vs-".join(slug(run_label(d)) for d in run_dirs)
    plot_runs(run_dirs, out, args.formats)


# --------------------------------------------------------------------------- battery

# Library names as written by the battery -> display names used in charts.
BATTERY_LIBS = {"automerge": "Automerge", "automerge-hashless": "Automerge hashless", "yrs": "Yrs"}


def load_battery(path: Path, label: str) -> list[dict]:
    """Rows from a `battery run` result file, with display library and run label."""
    data = json.loads(path.read_text())
    if data.get("suite") != "crdt-bench-battery":
        sys.exit(f"{path} is not a crdt-bench battery result (suite={data.get('suite')!r})")
    rows = []
    for r in data["results"]:
        base, lib = r["name"].rsplit("/", 1)
        base = r.get("base") or base
        lib = r.get("library") or lib
        rows.append({**r, "base": base, "lib": BATTERY_LIBS.get(lib, lib), "run": label})
    return rows


def battery_case(row: dict) -> str:
    """Benchmark name without its group prefix, for axis labels."""
    return row["base"].split("/", 1)[1] if "/" in row["base"] else row["base"]


def short_value(raw: float, unit_fn) -> str:
    """At most three significant digits in the value's own unit: 941ms, 45.5ms, 330µs."""
    unit, scale = unit_fn(raw)
    text = f"{raw / scale:.3g}"
    if "e" in text:
        text = f"{raw / scale:.0f}"
    sep = " " if unit[-1:] == "B" else ""
    return f"{text}{sep}{unit}"


def plot_battery_bars(plt, title, rows, value, err, xlabel_fmt, unit_fn, colors, multi, out, name, formats, unrunnable):
    """Horizontal grouped bars: one row per case, one bar per library."""
    cases = sorted({battery_case(r) for r in rows} | {battery_case(r) for r in unrunnable})
    names = list(dict.fromkeys(series_name(r, multi) for r in rows))
    vals = [value(r) for r in rows]
    unit, scale = unit_fn(max(vals))
    fig, ax = plt.subplots(figsize=(8.5, 1.4 + 0.32 * len(cases) * max(len(names), 1)))
    height = 0.8 / max(len(names), 1)
    for i, n in enumerate(names):
        lookup = {battery_case(r): r for r in rows if series_name(r, multi) == n}
        ys = [j - 0.4 + height * (i + 0.5) for j, c in enumerate(cases) if c in lookup]
        rs = [lookup[c] for c in cases if c in lookup]
        xs = [value(r) / scale for r in rs]
        xerr = None
        if err:
            xerr = [[(value(r) - err(r)[0]) / scale for r in rs], [(err(r)[1] - value(r)) / scale for r in rs]]
        hatch = HATCHES[RUN_INDEX.get(rs[0]["run"], 0) % len(HATCHES)] if rs else None
        ax.barh(ys, xs, height * 0.92, color=colors[n], hatch=hatch, edgecolor=SURFACE, lw=0, label=n,
                xerr=xerr, ecolor=INK_2, capsize=0, error_kw={"lw": 1})
        for y, r in zip(ys, rs):  # value label just past the bar or its whisker
            end = max(value(r), err(r)[1] if err else 0) / scale
            ax.annotate(short_value(value(r), unit_fn), (end, y), xytext=(4, 0), textcoords="offset points",
                        va="center", fontsize=7.5, color=INK)
    for r in unrunnable:  # say "n/a" instead of leaving a silent gap
        n = series_name(r, multi)
        if n in names:
            j, i = cases.index(battery_case(r)), names.index(n)
            label = " unsupported" if r["kind"] == "unsupported" else " n/a"
            ax.text(0, j - 0.4 + height * (i + 0.5), label, va="center", fontsize=8, color=INK_2)
    ax.set_yticks(range(len(cases)), cases)
    ax.invert_yaxis()
    if max(vals) / max(min(vals), 1e-9) >= 30:
        ax.set_xscale("log")
    left, right = ax.get_xlim()  # room for the value labels
    ax.set_xlim(left, right * (2.5 if ax.get_xscale() == "log" else 1.12))
    ax.set_xlabel(xlabel_fmt.format(unit=unit))
    ax.grid(axis="y", visible=False)
    ax.set_title(title, pad=12)
    if len(names) > 1:  # in the title row, so it never covers a bar
        ax.legend(loc="lower right", bbox_to_anchor=(1, 1), ncol=len(names), fontsize=9, frameon=False,
                  borderaxespad=0.2)
    ax.minorticks_off()
    if ax.get_xscale() == "log":
        from matplotlib.ticker import FuncFormatter

        ax.xaxis.set_major_formatter(FuncFormatter(lambda v, _: f"{v:g}"))
    fig.tight_layout()
    paths = save(fig, out, name, formats)
    plt.close(fig)
    return paths


def plot_battery_series(plt, base, rows, colors, multi, out, formats):
    """Per-step time for a series benchmark: faint raw points, bold rolling median."""
    import statistics

    unit, scale = time_unit(max(max(r["points"]) for r in rows))
    fig, ax = plt.subplots(figsize=(8.5, 4))
    for r in rows:
        n = series_name(r, multi)
        pts = [p / scale for p in r["points"]]
        w = max(len(pts) // 50, 1)
        smooth = [statistics.median(pts[max(0, i - w): i + w + 1]) for i in range(len(pts))]
        ls = LINESTYLES[RUN_INDEX.get(r["run"], 0) % len(LINESTYLES)]
        ax.plot(pts, color=colors[n], lw=0.6, alpha=0.25)
        ax.plot(smooth, color=colors[n], lw=2, ls=ls, label=n)
    if max(max(r["points"]) for r in rows) / max(min(min(r["points"]) for r in rows), 1) >= 100:
        ax.set_yscale("log")
        plain_log_labels(ax)
    ax.set_xlabel("step")
    ax.set_ylabel(f"time per step ({unit})")
    ax.set_title(base, pad=20)
    ax.text(0, 1.01, "thin: every step · bold: rolling median", transform=ax.transAxes, fontsize=8.5, color=INK_2, va="bottom")
    ax.legend(loc="upper left", fontsize=9)
    fig.tight_layout()
    paths = save(fig, out, slug(base), formats)
    plt.close(fig)
    return paths


# Lighter step of a palette slot, for a library's steady-state bar beside its peak.
TINTS = {"#2a78d6": "#86b6ef", "#eb6834": "#f6b393"}
# Eg-walker traces in the paper's order: sequential, asynchronous, concurrent.
EGWALKER_TRACES = ["S1", "S2", "S3", "A1", "A2", "C1", "C2"]


def tint(color: str) -> str:
    if color in TINTS:
        return TINTS[color]
    rgb = [int(color[i:i + 2], 16) for i in (1, 3, 5)]
    return "#" + "".join(f"{(c + 255) // 2:02x}" for c in rgb)


def memory_block(case: str) -> tuple[int, int]:
    """Generated loads, then Eg-walker loads, then Eg-walker clones (and anything else last)."""
    op = {"load": 0, "clone": 1}.get(case.split("/", 1)[0], 2)
    return op, int("egwalker_paper/" in case)


def memory_order(case: str) -> tuple:
    trace = case.rsplit("/", 1)[-1]
    rank = EGWALKER_TRACES.index(trace) if trace in EGWALKER_TRACES else -1
    return (*memory_block(case), rank, case)


def plot_memory_peak_vs_steady(plt, rows, colors, multi, out, formats):
    """Peak vs steady-state heap, one panel per library on a shared log axis.

    A long dark bar over a short light one is memory the operation needed only
    while it ran.
    """
    from matplotlib.ticker import FuncFormatter

    names = list(dict.fromkeys(series_name(r, multi) for r in rows))
    lookup = {(battery_case(r), series_name(r, multi)): r for r in rows}
    cases = sorted({battery_case(r) for r in rows}, key=memory_order)
    largest = max(max(r["peak_bytes"], r["steady_bytes"]) for r in rows)
    unit, scale = byte_unit(largest)
    smallest = min(max(min(r["peak_bytes"], r["steady_bytes"]), 1) for r in rows)
    # Shared, with room for the value labels past the longest bar.
    xlim = (smallest / scale / 2, largest / scale * 4)

    fig, axes = plt.subplots(1, len(names), figsize=(7 * len(names), 0.42 * len(cases) + 2.2),
                             sharey=True, squeeze=False)
    h = 0.38
    for ax, n in zip(axes[0], names):
        peak_c, steady_c = colors[n], tint(colors[n])
        for i, c in enumerate(cases):
            r = lookup.get((c, n))
            if r is None:
                continue
            for dy, key, color, label_color in ((-h / 2, "peak_bytes", peak_c, INK),
                                                (h / 2, "steady_bytes", steady_c, INK_2)):
                v = r[key] / scale
                ax.barh(i + dy, v, h, color=color, edgecolor=SURFACE, linewidth=1.5)
                ax.annotate(short_value(r[key], byte_unit), (v, i + dy), xytext=(4, 0),
                            textcoords="offset points", va="center", fontsize=8, color=label_color)
        ax.set_xscale("log")
        ax.set_xlim(*xlim)
        ax.xaxis.set_major_formatter(FuncFormatter(lambda v, _: f"{v:g}"))
        ax.minorticks_off()
        ax.grid(axis="y", visible=False)
        ax.set_xlabel(f"heap ({unit}, log scale)")
        ax.set_title(n, color=peak_c, fontsize=13)
        ax.legend(handles=[plt.Rectangle((0, 0), 1, 1, color=peak_c), plt.Rectangle((0, 0), 1, 1, color=steady_c)],
                  labels=["peak during the operation", "steady afterwards"], loc="lower right",
                  bbox_to_anchor=(1, 1), ncol=2, frameon=False, fontsize=9, borderaxespad=0.2)
        for i in range(1, len(cases)):  # rule between blocks of related benchmarks
            if memory_block(cases[i]) != memory_block(cases[i - 1]):
                ax.axhline(i - 0.5, color=GRID, lw=1)
    axes[0][0].set_yticks(range(len(cases)), cases)
    axes[0][0].invert_yaxis()
    fig.suptitle("Memory: peak during the operation vs steady state afterwards",
                 x=0.01, ha="left", fontsize=13.5, fontweight="bold", color=INK)
    fig.tight_layout()
    paths = save(fig, out, "memory-peak-vs-steady", formats)
    plt.close(fig)
    return paths


@dataclass
class Figure:
    title: str
    path: Path
    caption: str = ""


# Intro written above a slot's charts.
SLOT_INTROS = {
    "sampled": "One chart per group: the bar is the median, the whisker spans the fastest and slowest sample.",
}
# An empty or filled slot left by `battery report`, see figure_slot in src/battery/report.rs.
SLOT = re.compile(r"(<!-- figures:(?P<name>[^\n]+?) -->\n)(?P<body>.*?)(<!-- /figures:(?P=name) -->)", re.S)


def slot_body(name: str, figs: list[Figure], report_dir: Path, as_html: bool) -> str:
    def rel(p: Path) -> str:
        return Path(os.path.relpath(p.resolve(), report_dir.resolve())).as_posix()

    intro = SLOT_INTROS.get(name, "")
    if as_html:
        out = f"<p>{html.escape(intro)}</p>\n" if intro else ""
        for f in figs:
            cap = f"<figcaption>{html.escape(f.caption)}</figcaption>" if f.caption else ""
            out += (f'<figure class="chart"><img src="{html.escape(rel(f.path))}" '
                    f'alt="{html.escape(f.title)}">{cap}</figure>\n')
        return out
    out = f"{intro}\n\n" if intro else ""
    for f in figs:
        out += f"![{f.title}]({rel(f.path)})\n\n"
        if f.caption:
            out += f"{f.caption}\n\n"
    return out


def fill_report(report_dir: Path, slots: dict[str, list[Figure]]) -> None:
    """Put the charts into the figure slots of report.md / report.html written by `battery report`.

    Replaces whatever a slot already holds, so this can be re-run; a slot with
    no charts this time is emptied.
    """
    for name, as_html in (("report.md", False), ("report.html", True)):
        path = report_dir / name
        if not path.exists():
            continue
        text = path.read_text()
        filled = 0

        def repl(m: re.Match) -> str:
            nonlocal filled
            figs = slots.get(m["name"], [])
            filled += bool(figs)
            return m[1] + (slot_body(m["name"], figs, report_dir, as_html) if figs else "") + m[4]

        new, count = SLOT.subn(repl, text)
        if count == 0:
            print(f"{path} has no figure slots; rewrite it with `battery report` and re-run this")
            continue
        path.write_text(new)
        print(f"added charts to {filled} of {count} figure slot(s) in {path}")


def cmd_battery(args: argparse.Namespace) -> None:
    files = [Path(f) / "battery.json" if Path(f).is_dir() else Path(f) for f in args.results]
    multi = len(files) > 1
    labels = [f.resolve().parent.name if f.name == "battery.json" else f.stem for f in files]
    if multi and len(set(labels)) != len(labels):
        sys.exit(f"result files need distinct names to be compared, got {labels}")
    rows = [r for f, label in zip(files, labels) for r in load_battery(f, label)]
    out = Path(args.out) if args.out else files[0].parent / "plots"
    out.mkdir(parents=True, exist_ok=True)

    plt = setup_mpl()
    colors = Colors(list(dict.fromkeys(series_name(r, multi) for r in rows)))
    RUN_INDEX.clear()
    RUN_INDEX.update({label: i for i, label in enumerate(labels)})

    by_kind: dict[str, list[dict]] = defaultdict(list)
    for r in rows:
        by_kind[r["kind"]].append(r)
    unrunnable = [r for r in by_kind["not_run"] if r.get("reason")] + by_kind["unsupported"]
    figures: list[tuple[str, Path]] = []
    # Report figure slot -> charts for it, see fill_report.
    slots: dict[str, list[Figure]] = defaultdict(list)

    sampled_groups: dict[str, list[dict]] = defaultdict(list)
    for r in by_kind["sampled"]:
        sampled_groups[r["group"]].append(r)
    for group, rs in sorted(sampled_groups.items()):
        na = [r for r in unrunnable if r["group"] == group and r["benchmark_kind"] == "sampled"
              and r["base"] in {x["base"] for x in rs}]
        paths = plot_battery_bars(plt, group, rs, lambda r: r["median"], lambda r: (r["min"], r["max"]),
                                  "median time per iteration ({unit}) · whiskers: min–max", time_unit,
                                  colors, multi, out, slug(group), args.formats, na)
        figures.append((group, paths[0]))
        slots["sampled"].append(Figure(group, paths[0]))

    series_by_base: dict[str, list[dict]] = defaultdict(list)
    for r in by_kind["series"]:
        series_by_base[r["base"]].append(r)
    for base, rs in sorted(series_by_base.items()):
        path = plot_battery_series(plt, base, rs, colors, multi, out, args.formats)[0]
        figures.append((base, path))
        slots[f"series:{base}"].append(Figure(base, path))

    if by_kind["memory"]:
        for key, title, slot in (("peak_bytes", "Peak heap", "memory-peak"),
                                 ("steady_bytes", "Steady-state heap", "memory-steady")):
            paths = plot_battery_bars(plt, title, by_kind["memory"], lambda r, k=key: r[k], None,
                                      "heap ({unit})", byte_unit, colors, multi, out, slug(f"memory {key}"),
                                      args.formats, [])
            figures.append((title, paths[0]))
            slots[slot].append(Figure(title, paths[0]))
        paths = plot_memory_peak_vs_steady(plt, by_kind["memory"], colors, multi, out, args.formats)
        figures.append(("Peak vs steady-state heap", paths[0]))
        slots["memory-steady"].append(Figure(
            "Peak vs steady-state heap", paths[0],
            "Peak against steady state, per library: a long dark bar over a short light one is memory "
            "the operation needed only while it ran."))

    write_index(out, figures, files)
    print(f"\n{len(figures)} chart(s) -> {out}")
    if multi:
        print("comparing experiments: charts not added to any report")
    else:
        fill_report(files[0].parent, slots)


# --------------------------------------------------------------------------- CLI


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    r = sub.add_parser("run", help="run workloads, then plot")
    r.add_argument("workloads", nargs="+", metavar="WORKLOAD", help=f"{', '.join(WORKLOADS)} or all")
    r.add_argument("--sizes", help="CRDT_SIZES: comma-separated doc sizes for seq_add / load (e.g. 6000,60000)")
    r.add_argument("--file-sizes", help="CRDT_FILE_SIZES: data files for load / mem (e.g. 6k,60k,600k)")
    r.add_argument("--sample-size", type=int, help="CRDT_SAMPLE_SIZE: Criterion samples per benchmark (>= 10)")
    r.add_argument("--filter", help="only run benchmarks whose id contains this string (Criterion filter)")
    r.add_argument("--quick", action="store_true", help="pass --quick to Criterion")
    r.add_argument("--label", help="name for this run, e.g. am2 / am3; used as the legend when comparing")
    r.add_argument("--criterion-html", action="store_true", help="also let Criterion build its own HTML report")
    r.set_defaults(func=cmd_run)

    pl = sub.add_parser("plot", help="(re)plot one run, or compare several")
    pl.add_argument("runs", nargs="+", metavar="RUN_DIR")
    pl.add_argument("--out", help="output directory (default: <run>/plots, or results/compare/... for several runs)")
    pl.set_defaults(func=cmd_plot)

    b = sub.add_parser("battery", help="plot result files written by `cargo run --release --bin battery -- run`, "
                                       "and add the charts to the experiment's report")
    b.add_argument("results", nargs="*", metavar="EXPERIMENT", default=[str(RESULTS / "latest-battery")],
                   help="battery experiment directories (or their battery.json); several to compare "
                        "(default: results/latest-battery)")
    b.add_argument("--out", help="output directory (default: plots/ in the first experiment)")
    b.set_defaults(func=cmd_battery)

    for sp in (r, pl, b):
        sp.add_argument("--formats", default="png", type=lambda s: s.split(","), help="comma-separated, e.g. png,svg,pdf")

    args = p.parse_args()
    if getattr(args, "sample_size", None) is not None and args.sample_size < 10:
        p.error("Criterion requires --sample-size >= 10")
    args.func(args)


if __name__ == "__main__":
    main()
