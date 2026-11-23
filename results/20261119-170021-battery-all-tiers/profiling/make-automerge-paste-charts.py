"""Charts from the callgrind profile of one 100k-character Automerge paste + commit.

Numbers are copied from `callgrind_annotate --inclusive=yes automerge-paste-100k.callgrind.out` and the
call counts on the edges of automerge-paste-100k-callgraph.dot (gprof2dot).
"""
import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Patch

INK, MUTED, SURFACE, GRID = "#262624", "#6b6a63", "#ffffff", "#e6e5e0"
SPLICE, COMMIT, OTHER = "#2a78d6", "#eb6834", "#c9c8c1"  # validated categorical slots 1, 2
plt.rcParams.update({"font.size": 11, "text.color": INK, "axes.labelcolor": INK,
                     "xtick.color": MUTED, "ytick.color": INK, "axes.edgecolor": "#9a998f"})

# ----- 1. Where the instructions go (share of 437M instructions) -----
fig, ax = plt.subplots(figsize=(12, 4.4))
phases = [("splice_text", 75.08, SPLICE), ("commit", 24.02, COMMIT), (None, 0.82, OTHER)]
parts = [
    ("OpSet::splice", "insert the ops into the\nin-memory op set", 54.94, SPLICE),
    ("BatchInsertion::\nsplice_text", "create one op\nper character", 20.15, SPLICE),
    ("build_change", "encode the ops\ninto the change", 18.69, COMMIT),
    ("free ops", None, 3.54, COMMIT),
    ("SHA-256", None, 1.19, COMMIT),
    (None, None, 0.60, COMMIT),
    (None, None, 0.82, OTHER),
]
left = 0.0
for label, share, color in phases:
    ax.barh(1, share, left=left, height=0.8, color=color, edgecolor=SURFACE, linewidth=2)
    if label:
        ax.text(left + share / 2, 1, f"{label}\n{share:.1f}%", ha="center", va="center",
                color=SURFACE, fontsize=12, fontweight="bold")
    left += share
left = 0.0
small = []
for name, desc, share, color in parts:
    ax.barh(0, share, left=left, height=0.8, color=color, edgecolor=SURFACE, linewidth=2)
    mid = left + share / 2
    if name and share >= 10:
        ax.text(mid, 0, f"{name}\n{share:.1f}%", ha="center", va="center", color=SURFACE, fontsize=10.5,
                fontweight="bold", linespacing=1.1)
        ax.text(mid, -0.5, desc, ha="center", va="top", color=MUTED, fontsize=9.5, linespacing=1.15)
    elif name:
        small.append((name, share, mid))
    left += share
# The thin commit segments share one label and one leader line.
text = "  ·  ".join(f"{name} {share:.1f}%" for name, share, _ in small)
ax.annotate(text, xy=(small[0][2], -0.4), xytext=(100, -1.1), ha="right", va="center", fontsize=9.5,
            color=INK, arrowprops=dict(arrowstyle="-", color=MUTED, lw=0.9))
ax.set_yticks([1, 0], ["phase", "inside it"])
ax.set_xlim(0, 100)
ax.set_ylim(-1.35, 1.5)
ax.set_xlabel("share of all instructions (%)  ·  callgrind, one 100k-character paste + commit, automerge 0.12")
for side in ("top", "right", "left"):
    ax.spines[side].set_visible(False)
ax.tick_params(axis="y", length=0)
fig.suptitle("Automerge paste: 75% of the work happens before commit; hashing is 1.2%",
             x=0.01, ha="left", fontsize=13.5, fontweight="bold")
fig.tight_layout()
fig.savefig("automerge-paste-100k-cost-breakdown.png", dpi=150, facecolor=SURFACE)

# ----- 2. Work per pasted character (call counts / 100,000), split by phase -----
# (label, calls during splice_text, calls during commit)
calls = [
    ("RleState::append_n — column run-length encoder", 1_400_022, 800_008),
    ("SpliceWriter::push — write one column value", 1_300_021, 0),
    ("Edit::insert_run — edit one column", 1_100_017, 0),
    ("malloc", 300_098, 35),
    ("free", 200_257, 100_001),
    ("TxOp::insert_val — create the op", 100_000, 0),
]
fig, ax = plt.subplots(figsize=(11, 4.2))
labels = [c[0] for c in calls][::-1]
during_splice = [c[1] / 100_000 for c in calls][::-1]
during_commit = [c[2] / 100_000 for c in calls][::-1]
ax.barh(labels, during_splice, height=0.62, color=SPLICE, edgecolor=SURFACE, linewidth=2)
ax.barh(labels, during_commit, left=during_splice, height=0.62, color=COMMIT, edgecolor=SURFACE, linewidth=2)
for y, (s, c) in enumerate(zip(during_splice, during_commit)):
    total = s + c
    note = f"{total:.0f}×" + (f"  ({s:.0f} + {c:.0f} at commit)" if c >= 0.5 else "")
    ax.text(total + 0.3, y, note, va="center", color=INK, fontsize=10)
ax.set_xlabel("calls per pasted character  (Yrs: 11 allocations for the whole paste)")
ax.set_xlim(0, 30)
for side in ("top", "right"):
    ax.spines[side].set_visible(False)
ax.grid(axis="x", color=GRID)
ax.set_axisbelow(True)
ax.legend(handles=[Patch(color=SPLICE, label="during splice_text"), Patch(color=COMMIT, label="during commit")],
          loc="lower right", frameon=False, fontsize=10)
fig.suptitle("Automerge paste: work done for every single character", x=0.01, ha="left",
             fontsize=13.5, fontweight="bold")
fig.tight_layout()
fig.savefig("automerge-paste-100k-calls-per-char.png", dpi=150, facecolor=SURFACE)
print("wrote automerge-paste-100k-cost-breakdown.png, automerge-paste-100k-calls-per-char.png")
