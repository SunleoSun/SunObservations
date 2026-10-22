#[path = "mod.rs"]
mod support;

use std::{fs, path::Path, process::Command};

const FIXTURE_ENV: &str = "SUN_OBS_BENCHMARK_REPORT_FIXTURE";

#[test]
fn stores_reports_in_build_and_propagates_failures() {
    if std::env::var_os(FIXTURE_ENV).is_some() {
        support::run(|| {
            println!("case,value");
            println!("fixture,42");
            eprintln!("benchmark diagnostic");
            let temporary = support::tempdir().unwrap();
            let jsonl = temporary.path().join("records.jsonl");
            fs::write(&jsonl, "{}\n").unwrap();
            println!("temporary_jsonl,{}", jsonl.display());
            if std::env::var(FIXTURE_ENV).unwrap() == "failure" {
                panic!("requested benchmark failure");
            }
        });
        return;
    }

    for mode in ["success", "failure"] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "stores_reports_in_build_and_propagates_failures",
                "--nocapture",
            ])
            .env(FIXTURE_ENV, mode)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(if mode == "success" { 0 } else { 101 })
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        let directory = Path::new(
            stderr
                .lines()
                .find_map(|line| line.strip_prefix("Benchmark reports: "))
                .unwrap(),
        );
        assert!(directory.starts_with(support::output_directory().unwrap()));
        let metrics = fs::read_to_string(directory.join("metrics.csv")).unwrap();
        let diagnostics = fs::read_to_string(directory.join("stderr.txt")).unwrap();
        assert!(metrics.contains("case,value\nfixture,42\n"));
        assert!(stdout.contains("case,value\nfixture,42\n"));
        let temporary_jsonl = Path::new(
            metrics
                .lines()
                .find_map(|line| line.strip_prefix("temporary_jsonl,"))
                .unwrap(),
        );
        assert!(temporary_jsonl.starts_with(directory));
        assert!(!temporary_jsonl.exists());
        assert!(diagnostics.contains("benchmark diagnostic"));
        assert!(stderr.contains("benchmark diagnostic"));
        assert_eq!(
            diagnostics.contains("requested benchmark failure"),
            mode == "failure"
        );
    }
}
