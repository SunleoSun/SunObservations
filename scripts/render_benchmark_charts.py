#!/usr/bin/env python3
"""Render deterministic README SVGs from report-validated reference-machine metrics.

No network, third-party packages, benchmark execution or timing-dependent SVG metadata.
"""

import argparse
import csv
from decimal import Decimal
from html import escape
from pathlib import Path
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "scripts" / "benchmark_metrics.csv"
COLORS = ("#126b96", "#b55112")


def report_table(source, section):
    lines = (ROOT / source).read_text(encoding="utf-8").splitlines()
    heading = "## " + section
    start = lines.index(heading) + 1
    end = next((i for i in range(start, len(lines)) if lines[i].startswith("## ")), len(lines))
    table = [line for line in lines[start:end] if line.startswith("|")]
    if not table:
        raise ValueError(f"No evidence table: {source} / {section}")
    return [[cell.strip() for cell in line.strip("|").split("|")] for line in table]


def load_metrics():
    with DATA.open(encoding="utf-8", newline="") as handle:
        rows = list(csv.DictReader(handle))
    seen = set()
    for row in rows:
        identity = (row["chart"], row["case"], row["series"])
        if identity in seen:
            raise ValueError(f"Duplicate metric: {identity}")
        seen.add(identity)
        table = report_table(row["source"], row["section"])
        key = row["row_key"].split(";")
        matches = [cells for cells in table[2:] if cells[:len(key)] == key]
        if len(matches) != 1:
            raise ValueError(f"Ambiguous/missing evidence: {row}")
        measured = matches[0][table[0].index(row["column"])]
        if Decimal(measured) != Decimal(row["value_ns"]) or Decimal(measured) <= 0:
            raise ValueError(f"Evidence mismatch: {row}; report says {measured}")
    return rows


def label(x, y, value, size=14, anchor="start", color="#172b3a"):
    return (f'<text x="{x:.2f}" y="{y:.2f}" font-size="{size}" '
            f'text-anchor="{anchor}" fill="{color}">{escape(str(value))}</text>')


def panel(rows, y, title, unit, throughput=False):
    cases = list(dict.fromkeys(row["case"] for row in rows))
    series = list(dict.fromkeys(row["series"] for row in rows))
    values = {(row["case"], row["series"]): float(row["value_ns"]) for row in rows}
    if len(values) != len(cases) * len(series):
        raise ValueError(f"Incomplete chart panel: {title}")
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
    description = "; ".join(f'{r["case"]}, {r["series"]}: {r["value_ns"]} {unit}' for r in rows)
    parts = [label(24, 34, title, 23), label(24, 61, subtitle, 14)]
    y = 80
    if split_series:
        for series in dict.fromkeys(row["series"] for row in rows):
            section, y = panel([r for r in rows if r["series"] == series], y, series, f"{unit} (lower is better)")
            parts.extend(section)
    else:
        section, y = panel(rows, y, "", "Million calls/s (higher is better)" if throughput else f"{unit} (lower is better)", throughput)
        parts.extend(section)
    parts.append(label(24, y + 9, footer, 13))
    height = y + 36
    svg = chr(10).join([
        f'<svg xmlns="http://www.w3.org/2000/svg" width="960" height="{height}" '
        f'viewBox="0 0 960 {height}" role="img" aria-labelledby="title desc">',
        f'<title id="title">{escape(title)}</title>',
        f'<desc id="desc">{escape(subtitle + ". " + description + ". " + footer)}</desc>',
        f'<rect width="960" height="{height}" fill="#ffffff"/>',
        '<g font-family="sans-serif">',
        *parts,
        '</g>',
        '</svg>',
        '',
    ])
    ET.fromstring(svg)
    return svg


def render():
    rows = load_metrics()
    specs = {
        "human-structured": ("Human capability: structured-only control", "Same-machine before/after; 5 x 1M calls; render-capable type after, no human sinks", "Overlapping ranges; do not interpret small median changes as universal speedups. Source: docs/BENCHMARKS.md", False, False),
        "console-scaling": ("Console producer vs through-flush service", "Default formatting; file-backed production worker; 5 x 262144 records", "Not terminal-emulator capacity. More producers can grow an unbounded backlog. Source: docs/BENCHMARKS.md", False, True),
        "console-rendering": ("Console default vs custom numeric row", "Through-flush ns/call; different output bytes and formatting threads; median of 5", "File-backed worker, not terminal painting. Row-only renderer: 595.38 ns. Source: docs/BENCHMARKS.md", False, False),
        "hot-path": ("Warm observation hot path", "No sink; Event payload; median of 3 x 1M macro calls", "EveryN(100) admits 1%; Always includes payload serialization. Source: docs/BENCHMARKS.md", False, False),
        "selector-scale": ("Warm selector cost", "N irrelevant rules per category (4N total); fixed observation depth and tags", "Separate panel scales. Setup and cold resolution excluded. Source: docs/BENCHMARKS.md", True, False),
        "jsonl-scaling": ("JSONL producer vs end-to-end throughput", "One shared sink; 1M calls total; median of 3 samples; throughput = 1000 / aggregate ns/call", "End-to-end includes flush_channels(), not fsync. Source: docs/BENCHMARKS.md", False, True),
        "sink-comparison": ("Accepted-observation sink comparison", "One producer; Event payload; median of 3 x 1M macro calls", "No-sink control still serializes payload; fan-out uses 4 MemorySinks. Source: docs/BENCHMARKS.md", False, False),
        "writer-batch": ("Bounded JSONL writer batch tradeoff", "End-to-end aggregate ns/record; 262144 records/case; median of 10 samples", "8192 bounds the writer chunk, not the queue. Source: docs/BATCH_BENCHMARKS.md", False, False, "ns/record"),
    }
    if {r["chart"] for r in rows} != set(specs):
        raise ValueError("Dataset/chart specifications disagree")
    return {f"benchmark-{key}.svg": chart([r for r in rows if r["chart"] == key], *spec)
            for key, spec in specs.items()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Validate evidence and byte-compare assets without writing")
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
    print(f"Validated {len(assets)} SVGs against tracked benchmark report tables.")


if __name__ == "__main__":
    main()
