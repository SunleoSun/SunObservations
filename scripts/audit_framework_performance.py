"""Build/replay native framework benchmarks; optional paired ABBA comparison."""
import argparse
import csv
import json
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess

SUITES = {
    "generative": ("generative_certifications", {}, ["--measure"]),
    "negative": ("generative_certifications", {}, ["--measure", "--audit"]),
    "analysis": ("analysis", {}, ["--measure"]),
    "hot": ("runtime", {"SUN_OBS_BENCH_GROUP": "hot"}, ["--measure"]),
    "selectors": ("runtime", {"SUN_OBS_BENCH_GROUP": "selectors"}, ["--measure"]),
    "sink_threads": ("runtime", {"SUN_OBS_BENCH_GROUP": "threads"}, ["--measure"]),
    "allocations": ("investigation_allocations", {}, ["--measure"]),
    **{group: ("framework_audit", {"SUN_AUDIT_GROUP": group}, ["--measure"])
       for group in ["emission", "capture", "combined", "threads", "lag", "simd"]},
}
SUITES["storage"] = ("capture", {}, ["--measure"])
MEASUREMENTS = {"milliseconds", "producer_ns_per_call", "e2e_ns_per_call", "total_ms",
                "ns_per_record", "allocations", "reallocations", "deallocations", "requested_bytes"}


def verify_samples(rows):
    if not rows:
        raise RuntimeError("benchmark produced no metrics")
    groups = {}
    for row in rows:
        key = tuple((k, v) for k, v in row.items() if k not in MEASUREMENTS | {"sample"})
        groups.setdefault(key, []).append(row.get("sample"))
    expected = ["0", "1", "2"] if "sample" in rows[0] else [None]
    for key, samples in groups.items():
        if sorted(samples, key=str) != expected:
            raise RuntimeError(f"benchmark samples incomplete or duplicated: {key}: {samples}")


def command(args, env):
    completed = subprocess.run(args, env=env, text=True, capture_output=True)
    if completed.returncode:
        print(completed.stdout, flush=True)
        print(completed.stderr, flush=True)
        raise RuntimeError(f"command failed ({completed.returncode}): {args}")
    return completed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target-dir", type=Path)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--build-only", action="store_true")
    parser.add_argument("--suites", nargs="+", choices=sorted(SUITES), default=list(SUITES))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    for key in ["SUN_CERT_SEED", "SUN_CERT_CASES_BUDGET", "SUN_CERT_REPORT", "SUN_OBS_BENCH_GROUP", "SUN_OBS_BENCH_REPEATS", "SUN_AUDIT_GROUP", "SUN_OBS_INTERNAL_BENCHMARK_RUN_DIRECTORY"]:
        env.pop(key, None)
    if args.target_dir:
        env["CARGO_TARGET_DIR"] = str(args.target_dir.resolve())
    if args.manifest:
        current = json.loads(args.manifest.read_text())
    else:
        benches = sorted({SUITES[suite][0] for suite in args.suites})
        build_args = ["cargo", "test", "--locked", "--release", "-p", "sun-observations", "--no-run", "--message-format=json"]
        for bench in benches:
            build_args += ["--bench", bench]
        print("Building native benchmarks", flush=True)
        build = command(build_args, env)
        artifacts = [json.loads(line) for line in build.stdout.splitlines() if line.startswith("{")]
        executables = {a["target"]["name"]: a["executable"] for a in artifacts if a.get("reason") == "compiler-artifact" and a.get("executable")}
        if set(benches) - executables.keys():
            raise RuntimeError("Cargo did not produce every requested executable")
        current = {"executables": executables, "rustc": command(["rustc", "-Vv"], env).stdout,
                   "git_head": command(["git", "rev-parse", "HEAD"], env).stdout.strip(),
                   "git_status": command(["git", "status", "--short"], env).stdout,
                   "platform": platform.platform(), "processor": platform.processor(), "logical_cpus": os.cpu_count(),
                   "build_command": build_args, "target_dir": env.get("CARGO_TARGET_DIR")}
    (args.output / "executables.json").write_text(json.dumps(current, indent=2) + "\n")
    if args.build_only:
        return
    baseline = json.loads(args.baseline.read_text()) if args.baseline else None
    rows = []
    reports = []
    schedule = [("before", baseline), ("after", current), ("after", current), ("before", baseline)] if baseline else [("current", current), ("current", current)]
    for suite in args.suites:
        bench, settings, flags = SUITES[suite]
        for round_index, (phase, manifest) in enumerate(schedule):
            print(f"{suite}: {phase} ({round_index + 1}/{len(schedule)})", flush=True)
            process = command([manifest["executables"][bench], *flags], {**env, **settings})
            match = re.search(r"Benchmark reports: (.+)", process.stderr)
            if match is None:
                raise RuntimeError("benchmark did not produce a native report directory")
            directory = Path(match.group(1).strip())
            diagnostic = (directory / "stderr.txt").read_text()
            if diagnostic.strip():
                print(diagnostic, flush=True)
            native = list(csv.DictReader((directory / "metrics.csv").open(newline="")))
            verify_samples(native)
            rows += [{"suite": suite, "phase": phase, "round": round_index, **row} for row in native]
            reports.append({"suite": suite, "phase": phase, "round": round_index, "directory": str(directory), "rows": len(native)})
    fields = list(dict.fromkeys(key for row in rows for key in row))
    with (args.output / "metrics.csv").open("w", newline="") as out:
        writer = csv.DictWriter(out, fields)
        writer.writeheader()
        writer.writerows(rows)
    (args.output / "reports.json").write_text(json.dumps(reports, indent=2) + "\n")
    ignored = MEASUREMENTS | {"sample", "phase", "round"}
    groups = {}
    for row in rows:
        key = tuple((k, v) for k, v in row.items() if k not in ignored)
        groups.setdefault(key, {}).setdefault(row["phase"], []).append(row)
    summary = []
    for key, phases in groups.items():
        if baseline and set(phases) != {"before", "after"}:
            raise RuntimeError(f"semantic work or configuration differs across phases: {key}")
        if baseline and len(phases["before"]) != len(phases["after"]):
            raise RuntimeError(f"benchmark sample counts differ across phases: {key}")
        for metric in sorted(MEASUREMENTS):
            if metric not in next(iter(phases.values()))[0]:
                continue
            result = {**dict(key), "metric": metric}
            for phase, samples in phases.items():
                values = [float(row[metric]) for row in samples]
                result[phase] = statistics.median(values)
                result[phase + "_min"] = min(values)
                result[phase + "_max"] = max(values)
                result[phase + "_samples"] = len(values)
            if baseline:
                result["speedup"] = result["before"] / result["after"] if result["after"] else None
            summary.append(result)
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"Verified {len(rows)} raw rows, {len(summary)} median comparisons: {args.output}", flush=True)


if __name__ == "__main__":
    main()
