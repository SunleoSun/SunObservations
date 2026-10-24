"""Run and structurally verify the release generative benchmark; never impose timing gates."""
import argparse
import csv
import io
import os
import platform
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--summary", action="store_true", help="print selected sample-0 rows instead of the full raw CSV")
    options = parser.parse_args()
    command = ["cargo", "test", "--release", "-p", "sun-observations", "--bench", "generative_certifications", "--", "--measure"]
    env = dict(os.environ, SUN_CERT_SEED="184729361", SUN_CERT_CASES_BUDGET="64", SUN_CERT_REPORT="0")
    run = subprocess.run(command, text=True, capture_output=True, env=env, check=False)
    if run.returncode:
        print("Partial benchmark output discarded; last rows:")
        print("\n".join(run.stdout.splitlines()[-10:]))
        print(run.stderr, file=sys.stderr)
        raise SystemExit(run.returncode)
    rows = list(csv.DictReader(io.StringIO(run.stdout)))
    required = {
        "domain_compile", "planner_next_materialize", "planner_pair_target_selection",
        "target_solve", "input_materialize", "planner_higher_order_target_selection",
        "planned_report", "range_explore", "range_target_selection", "full_planned",
        "planned_check_evaluation", "checks_real_cases", "negative_exploration",
        "collection_explore", "collection_target_solve", "collection_materialize", "classification", "data_source_prefill",
        "failure_hard_stop", "planned_plain_result", "planned_captured_run", "capture_only",
    }
    missing = required - {r["operation"] for r in rows}
    if missing:
        raise RuntimeError(f"missing benchmark operations: {sorted(missing)}")
    if any(None in r or any(v is None for v in r.values()) for r in rows):
        raise RuntimeError("incomplete CSV rows")
    for row in rows:
        if not float(row["milliseconds"]) >= 0 or int(row["iterations"]) != 5:
            raise RuntimeError(f"invalid measurement row: {row}")
        op, shape = row["operation"], row["shape"]
        if op == "failure_hard_stop":
            executed = int(row["executed"])
            valid_count = executed == 1 if shape == "budget_1" else 5 <= executed < 4096 if shape == "budget_5" else executed == 4096
            if not valid_count or (row["stopped"] == "true") != (shape != "no_failures"):
                raise RuntimeError(f"hard stop artifact mismatch: {row}")
        if (op == "full_planned" and shape == "enum_noop") or op in {"planned_plain_result", "planned_captured_run"}:
            if row["executed"] != row["budget"]:
                raise RuntimeError(f"execution count mismatch: {row}")
    groups = {}
    dimensions = [k for k in rows[0] if k not in {"sample", "milliseconds"}]
    for row in rows:
        key = tuple(row[k] for k in dimensions)
        groups.setdefault(key, set()).add(row["sample"])
    if any(samples != {"0", "1", "2"} for samples in groups.values()):
        raise RuntimeError("unstable counters or missing samples")
    print("Command: " + " ".join(command))
    print("Platform: " + platform.platform())
    print("Processor: " + platform.processor())
    print("Logical CPUs: " + str(os.cpu_count()))
    print("Python: " + platform.python_version())
    print("Rust: " + subprocess.check_output(["rustc", "--version"], text=True).strip())
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], text=True).strip())
    print("Revision: " + revision + (" + dirty working tree" if dirty else ""))
    print("Environment: SUN_CERT_SEED=184729361 SUN_CERT_CASES_BUDGET=64 SUN_CERT_REPORT=0")
    print(f"Verified: {len(rows)} rows; {len(required)} operation families; {len(groups)} configurations; all three samples present; execution/hard-stop counters checked")
    print("Dataset: selected sample-0 rows" if options.summary else "Dataset: complete raw CSV")
    print("CSV_BEGIN")
    writer = csv.DictWriter(sys.stdout, fieldnames=list(rows[0]), lineterminator="\n")
    writer.writeheader()
    for row in rows:
        selected = row["sample"] == "0" and (
            row["operation"] in {"failure_hard_stop", "planned_plain_result", "planned_captured_run", "full_planned", "classification", "data_source_prefill", "checks_real_cases", "negative_exploration", "capture_only"}
            or row["operation"] in {"domain_compile", "planner_next_materialize", "planner_pair_target_selection", "target_solve", "input_materialize"} and row["shape"] == "enum_product" and row["dimensions"] == "4" and row["budget"] == "64"
        )
        if not options.summary or selected:
            writer.writerow(row)
    print("CSV_END")
    print(run.stderr.strip())


if __name__ == "__main__":
    main()
