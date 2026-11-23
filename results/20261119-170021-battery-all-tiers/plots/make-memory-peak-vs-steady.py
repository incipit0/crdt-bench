"""Peak vs steady-state heap per benchmark, one panel per library.

Usage: python peak_vs_steady.py <battery.json> <output.png>
"""
import json
import sys

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.ticker import FuncFormatter

INK, MUTED, GRID, SURFACE = "#262624", "#6b6a63", "#e6e5e0", "#ffffff"
# (peak, steady) per library: the categorical slot and a lighter step of the same hue.
COLORS = {"automerge": ("#2a78d6", "#86b6ef"), "yrs": ("#eb6834", "#f6b393")}
TRACES = ["S1", "S2", "S3", "A1", "A2", "C1", "C2"]
ORDER = (["load/typing", "load/big-paste", "load/text-splice-100"]
         + [f"load/egwalker_paper/{t}" for t in TRACES]
         + [f"clone/egwalker_paper/{t}" for t in TRACES])

results = json.load(open(sys.argv[1]))["results"]
mem = {(r["base"].removeprefix("memory/"), r["library"]): r for r in results if r["kind"] == "memory"}
rows = [b for b in ORDER if (b, "automerge") in mem and (b, "yrs") in mem]


def mib(b):
    return b / 2**20


def fmt(v):
    return f"{v * 1024:.0f} KiB" if v < 1 else f"{v:.3g} MiB"


plt.rcParams.update({"font.size": 10.5, "text.color": INK, "axes.labelcolor": INK,
                     "xtick.color": MUTED, "ytick.color": INK, "axes.edgecolor": "#9a998f"})
fig, axes = plt.subplots(1, 2, figsize=(14, 0.42 * len(rows) + 2.2), sharey=True)
h = 0.38
for ax, lib in zip(axes, ["automerge", "yrs"]):
    peak_c, steady_c = COLORS[lib]
    for i, b in enumerate(rows):
        r = mem[(b, lib)]
        p, s = mib(r["peak_bytes"]), mib(r["steady_bytes"])
        ax.barh(i - h / 2, p, h, color=peak_c, edgecolor=SURFACE, linewidth=1.5)
        ax.barh(i + h / 2, s, h, color=steady_c, edgecolor=SURFACE, linewidth=1.5)
        ax.text(p * 1.12, i - h / 2, fmt(p), va="center", fontsize=8.5, color=INK)
        ax.text(s * 1.12, i + h / 2, fmt(s), va="center", fontsize=8.5, color=MUTED)
    ax.set_xscale("log")
    ax.set_xlim(0.05, 3000)
    ax.xaxis.set_major_formatter(FuncFormatter(lambda v, _: f"{v:g}"))
    ax.grid(axis="x", color=GRID)
    ax.set_axisbelow(True)
    for side in ("top", "right"):
        ax.spines[side].set_visible(False)
    ax.set_xlabel("heap (MiB, log scale)")
    name = "Automerge" if lib == "automerge" else "Yrs"
    ax.set_title(name, loc="left", fontsize=13, fontweight="bold", color=peak_c)
    ax.legend(handles=[plt.Rectangle((0, 0), 1, 1, color=peak_c), plt.Rectangle((0, 0), 1, 1, color=steady_c)],
              labels=["peak during the operation", "steady afterwards"],
              loc="lower right", bbox_to_anchor=(1, 1.0), ncol=2, frameon=False, fontsize=9,
              borderaxespad=0.2)  # in the title row, so it never covers a bar
    # separate the three blocks: generated loads, egwalker loads, egwalker clones
    for y in (2.5, 9.5):
        ax.axhline(y, color=GRID, lw=1)
axes[0].set_yticks(range(len(rows)), rows)
axes[0].invert_yaxis()
fig.suptitle("Memory: Automerge's heap spikes while loading, then settles; Yrs stays close to its final size",
             x=0.01, ha="left", fontsize=13.5, fontweight="bold")
fig.tight_layout()
fig.savefig(sys.argv[2], dpi=140, facecolor=SURFACE)
print("wrote", sys.argv[2], f"({len(rows)} benchmarks)")
