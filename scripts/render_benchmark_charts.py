#!/usr/bin/env python3
"""Render deterministic README SVGs from tracked benchmark evidence.

No network, third-party packages, benchmark execution, or timing-dependent SVG metadata.
"""

import argparse
import csv
from decimal import Decimal
from html import escape
from pathlib import Path
import statistics
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
RUNTIME_DATA = ROOT / "scripts" / "benchmark_metrics.csv"
CAPTURE_DATA = ROOT / "scripts" / "capture_metrics.csv"
GENERATIVE_DATA = ROOT / "scripts" / "generative_certification_metrics.csv"
CERTIFICATION_DATA = ROOT / "scripts" / "certification_metrics.csv"
COLORS = ("#126b96", "#b55112", "#5e8c31")





def load_runtime_metrics():
    with RUNTIME_DATA.open(encoding="utf-8", newline="") as handle:
        rows = list(csv.DictReader(handle))
    if not rows:
        raise ValueError("Runtime benchmark dataset is empty")
    seen = set()
    for item in rows:
        identity = (item["chart"], item["case"], item["series"])
        if identity in seen:
            raise ValueError(f"Duplicate runtime metric: {identity}")
        seen.add(identity)
        value = Decimal(item["value_ns"])
        if not value.is_finite() or value <= 0:
            raise ValueError(f"Invalid runtime metric: {item}")
    return rows


def load_capture_medians():
    with CAPTURE_DATA.open(encoding="utf-8", newline="") as handle:
        source = list(csv.DictReader(handle))
    groups = {}
    for row in source:
        key = (row["operation"], int(row["records"]))
        groups.setdefault(key, []).append((int(row["sample"]), float(row["milliseconds"])))
    for key, values in groups.items():
        samples = sorted(sample for sample, _ in values)
        if samples != [0, 1, 2]:
            raise ValueError(f"Capture dataset must retain samples 0,1,2 for {key}: {samples}")
    return {key: statistics.median(value for _, value in values) for key, values in groups.items()}


def load_generative_rows():
    with GENERATIVE_DATA.open(encoding="utf-8", newline="") as handle:
        rows = list(csv.DictReader(handle))
    if not rows:
        raise ValueError("Generative benchmark dataset is empty")
    return rows
def load_certification_medians():
    with CERTIFICATION_DATA.open(encoding="utf-8", newline="") as handle:
        source = list(csv.DictReader(handle))
    groups = {}
    for item in source:
        key = (item["operation"], int(item["records"]))
        groups.setdefault(key, []).append((int(item["sample"]), float(item["milliseconds"])))
    for key, values in groups.items():
        samples = sorted(sample for sample, _ in values)
        if samples != [0, 1, 2]:
            raise ValueError(f"Certification dataset must retain samples 0,1,2 for {key}: {samples}")
    return {key: statistics.median(value for _, value in values) for key, values in groups.items()}


def row(case, series, value):
    return {"case": str(case), "series": series, "value_ns": f"{value:.6f}"}


def label(x, y, value, size=14, anchor="start", color="#172b3a"):
    return (
        f'<text x="{x:.2f}" y="{y:.2f}" font-size="{size}" '
        f'text-anchor="{anchor}" fill="{color}">{escape(str(value))}</text>'
    )


def panel(rows, y, title, unit, throughput=False):
    cases = list(dict.fromkeys(item["case"] for item in rows))
    series = list(dict.fromkeys(item["series"] for item in rows))
    values = {(item["case"], item["series"]): float(item["value_ns"]) for item in rows}
    if len(values) != len(cases) * len(series):
        raise ValueError(f"Incomplete chart panel: {title}")
    if len(series) > len(COLORS):
        raise ValueError(f"Too many chart series for palette: {series}")
    if throughput:
        values = {key: 1000 / value for key, value in values.items()}
    maximum = max(values.values()) * 1.18
    left, width = 255, 575
    top = y + 66
    step = 31 * len(series) + 15
    bottom = top + step * len(cases)
    parts = [label(24, y + 22, title, 17)]
    for i, name in enumerate(series):
        lx = 255 + i * 290
        parts.append(f'<rect x="{lx}" y="{y + 34}" width="12" height="12" fill="{COLORS[i]}"/>')
        parts.append(label(lx + 18, y + 45, name, 13))
    for tick in range(5):
        value = maximum * tick / 4
        x = left + width * tick / 4
        parts.append(f'<path d="M{x:.2f} {top - 8}V{bottom - 8}" stroke="#dce4e9"/>')
        parts.append(label(x, bottom + 12, f"{value:.1f}", 12, "middle"))
    parts.append(label(left + width / 2, bottom + 35, unit, 13, "middle"))
    for index, case in enumerate(cases):
        group_y = top + index * step
        parts.append(label(left - 15, group_y + (len(series) * 31) / 2 + 3, case, 14, "end"))
        for j, name in enumerate(series):
            value = values[(case, name)]
            bar_width = value / maximum * width
            bar_y = group_y + j * 31
            parts.append(f'<rect x="{left}" y="{bar_y}" width="{bar_width:.2f}" height="21" fill="{COLORS[j]}"/>')
            parts.append(label(left + bar_width + 7, bar_y + 16, f"{value:.2f}", 13))
    return parts, bottom + 60


def chart(rows, title, subtitle, footer, split_series=False, throughput=False, unit="ns/call"):
    description = "; ".join(
        f'{item["case"]}, {item["series"]}: {item["value_ns"]} {unit}' for item in rows
    )
    parts = [label(24, 34, title, 23), label(24, 61, subtitle, 14)]
    y = 80
    if split_series:
        for series in dict.fromkeys(item["series"] for item in rows):
            section, y = panel(
                [item for item in rows if item["series"] == series],
                y,
                series,
                f"{unit} (lower is better)",
            )
            parts.extend(section)
    else:
        section, y = panel(
            rows,
            y,
            "",
            "Million calls/s (higher is better)" if throughput else f"{unit} (lower is better)",
            throughput,
        )
        parts.extend(section)
    parts.append(label(24, y + 9, footer, 13))
    height = y + 36
    svg = chr(10).join(
        [
            f'<svg xmlns="http://www.w3.org/2000/svg" width="960" height="{height}" '
            f'viewBox="0 0 960 {height}" role="img" aria-labelledby="title desc">',
            f'<title id="title">{escape(title)}</title>',
            f'<desc id="desc">{escape(subtitle + ". " + description + ". " + footer)}</desc>',
            f'<rect width="960" height="{height}" fill="#ffffff"/>',
            '<g font-family="sans-serif">',
            *parts,
            "</g>",
            "</svg>",
            "",
        ]
    )
    ET.fromstring(svg)
    return svg


def runtime_assets():
    rows = load_runtime_metrics()
    specs = {
        "human-structured": (
            "Human capability: structured-only control",
            "Same-machine before/after; 5 x 1M calls; render-capable type after, no human sinks",
            "Overlapping ranges; do not interpret small median changes as universal speedups. Source: scripts/benchmark_metrics.csv",
            False,
            False,
        ),
        "console-scaling": (
            "Console producer vs through-flush service",
            "Default formatting; file-backed production worker; 5 x 262144 records",
            "Not terminal-emulator capacity. More producers can grow an unbounded backlog. Source: scripts/benchmark_metrics.csv",
            False,
            True,
        ),
        "console-rendering": (
            "Console default vs custom numeric row",
            "Through-flush ns/call; different output bytes and formatting threads; median of 5",
            "File-backed worker, not terminal painting. Row-only renderer: 595.38 ns. Source: scripts/benchmark_metrics.csv",
            False,
            False,
        ),
        "hot-path": (
            "Warm observation hot path",
            "No sink; Event payload; median of 3 x 1M macro calls",
            "EveryN(100) admits 1%; Always includes payload serialization. Source: scripts/benchmark_metrics.csv",
            False,
            False,
        ),
        "selector-scale": (
            "Warm selector cost",
            "N irrelevant rules per category (4N total); fixed observation depth and tags",
            "Separate panel scales. Setup and cold resolution excluded. Source: scripts/benchmark_metrics.csv",
            True,
            False,
        ),
        "jsonl-scaling": (
            "JSONL producer vs end-to-end throughput",
            "One shared sink; 1M calls total; median of 3 samples; throughput = 1000 / aggregate ns/call",
            "End-to-end includes flush_channels(), not fsync. Source: scripts/benchmark_metrics.csv",
            False,
            True,
        ),
        "sink-comparison": (
            "Accepted-observation sink comparison",
            "One producer; Event payload; median of 3 x 1M macro calls",
            "No-sink control still serializes payload; fan-out uses 4 MemorySinks. Source: scripts/benchmark_metrics.csv",
            False,
            False,
        ),
        "writer-batch": (
            "Bounded JSONL writer batch tradeoff",
            "End-to-end aggregate ns/record; 262144 records/case; median of 10 samples",
            "8192 bounds the writer chunk, not the queue. Source: scripts/benchmark_metrics.csv",
            False,
            False,
            "ns/record",
        ),
    }
    if {item["chart"] for item in rows} != set(specs):
        raise ValueError("Runtime dataset/chart specifications disagree")
    return {
        f"benchmark-{key}.svg": chart(
            [item for item in rows if item["chart"] == key], *spec
        )
        for key, spec in specs.items()
    }


def capture_assets():
    medians = load_capture_medians()
    scales = [(1000, "1k"), (10000, "10k"), (100000, "100k"), (1000000, "1M")]
    emission = []
    reuse = []
    for records, label_text in scales:
        emission.extend(
            [
                row(label_text, "Direct configured", medians[("direct_configured_emission", records)]),
                row(label_text, "Scoped capture", medians[("scoped_capture_emission", records)]),
            ]
        )
        reuse.extend(
            [
                row(label_text, "Prepare once", medians[("prepare_once_six_checks", records)]),
                row(label_text, "Reprepare", medians[("reprepare_six_checks", records)]),
            ]
        )
    return {
        "benchmark-capture-emission.svg": chart(
            emission,
            "Scoped capture follows direct emission cost",
            "Median of 3 release samples; identical small typed payload and bounded in-memory retention",
            "Capture finalization is measured separately; this is not a universal payload-size bound.",
            unit="ms",
        ),
        "benchmark-prepared-reuse.svg": chart(
            reuse,
            "Prepared evidence avoids repeated work",
            "Six numeric requests over the same correspondence; median of 3 release samples",
            "At 1M rows: 121.26 ms prepare-once versus 357.87 ms repeated preparation.",
            unit="ms",
        ),
    }

def certification_assets():
    medians = load_certification_medians()
    scales = [(1000, "1k"), (10000, "10k"), (100000, "100k"), (1000000, "1M")]
    decode = [
        row(label_text, "Typed Observation decode", medians[("observation_decode", records)])
        for records, label_text in scales
    ]
    lag = []
    for records, label_text in scales:
        lag.extend(
            [
                row(label_text, "Lag ±2", medians[("lag_pm2", records)]),
                row(label_text, "Lag ±8", medians[("lag_pm8", records)]),
                row(label_text, "Lag ±32", medians[("lag_pm32", records)]),
            ]
        )
    return {
        "benchmark-observation-decode.svg": chart(
            decode,
            "Typed Observation decode scaling",
            "Strict typed decode from retained Observation records; median of 3 release samples",
            "Decode cost includes serde conversion and source-coordinate construction. Source: scripts/certification_metrics.csv",
            unit="ms",
        ),
        "benchmark-lag-window.svg": chart(
            lag,
            "Lag search cost grows with candidate window",
            "Keyed Pearson lag search; median of 3 release samples",
            "The ±32 search evaluates many more candidate alignments than ±2. Source: scripts/certification_metrics.csv",
            unit="ms",
        ),
    }


def generative_assets():
    rows = load_generative_rows()
    budget_rows = [
        item
        for item in rows
        if item["operation"] == "full_planned" and item["shape"] == "enum_noop"
    ]
    budget_rows.sort(key=lambda item: int(item["budget"]))
    if [int(item["budget"]) for item in budget_rows] != [8, 64, 256, 1024]:
        raise ValueError("Missing canonical full_planned enum budget rows")
    budget = [
        row(item["budget"], "Full planned execution", float(item["milliseconds"]))
        for item in budget_rows
    ]

    potential_order = [256, 4096, 65536, 1048576]
    compile_rows = {
        int(item["potential"]): item
        for item in rows
        if item["operation"] == "domain_compile"
        and item["shape"] == "enum_product"
        and int(item["budget"]) == 64
        and int(item["dimensions"]) == 4
    }
    traverse_rows = {
        int(item["potential"]): item
        for item in rows
        if item["operation"] == "planner_next_materialize"
        and item["shape"] == "enum_product"
        and int(item["budget"]) == 64
        and int(item["dimensions"]) == 4
    }
    frontier = []
    for potential in potential_order:
        frontier.extend(
            [
                row(f"{potential:,}", "Compile", float(compile_rows[potential]["milliseconds"])),
                row(f"{potential:,}", "Traverse <=64", float(traverse_rows[potential]["milliseconds"])),
            ]
        )
    return {
        "benchmark-domain-budget.svg": chart(
            budget,
            "Generated execution is bounded by case budget",
            "Eight 32-choice enums; positive Cartesian potential about 1.1 trillion cases",
            "Recorded sample-0 rows are arithmetic means of five timed operations; work grows with requested budget, not full Cartesian size.",
            unit="ms",
        ),
        "benchmark-domain-frontier.svg": chart(
            frontier,
            "Domain planning stays frontier-bounded",
            "Four active enum dimensions, case budget 64; potential grows from 256 to 1,048,576",
            "Compile remains about 0.25-0.31 ms and traversal about 0.96-1.83 ms for these fixtures; difficult constraints can still cost more.",
            unit="ms",
        ),
    }


def render():
    assets = {}
    for group in (runtime_assets(), capture_assets(), certification_assets(), generative_assets()):
        overlap = assets.keys() & group.keys()
        if overlap:
            raise ValueError(f"Duplicate chart names: {sorted(overlap)}")
        assets.update(group)
    return assets


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="Validate tracked evidence and byte-compare assets without writing",
    )
    args = parser.parse_args()
    assets = render()
    destination = ROOT / "img"
    if not args.check:
        destination.mkdir(exist_ok=True)
    for name, svg in assets.items():
        path = destination / name
        if args.check:
            if not path.is_file() or path.read_bytes() != svg.encode("utf-8"):
                raise SystemExit(f"Missing or stale chart: {path}; run {Path(__file__).name}")
        else:
            path.write_bytes(svg.encode("utf-8"))
        print(f'{"Verified" if args.check else "Rendered"}: img/{name}')
    print(f"Validated {len(assets)} SVGs against tracked benchmark evidence.")


if __name__ == "__main__":
    main()
