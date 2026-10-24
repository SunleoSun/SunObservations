use std::process::Command;
use sun_observations::certifications::*;
#[derive(DomainInput)]
struct Input {
    value: i32,
}
const MODE: &str = "SUN_CERT_PROCESS_FIXTURE";
#[test]
fn planned_process_contract() {
    if let Ok(mode) = std::env::var(MODE) {
        let mut c = certification("process contract").executor(|i: &Input| -> Result<i32, ()> {
            if mode == "panic" {
                panic!("production panic")
            }
            if (1..=9).contains(&i.value) {
                Ok(i.value)
            } else {
                Err(())
            }
        });
        c.domain(domain!(Input{value.in_range(1..=9)}));
        c.failure_budget(2);
        let case = c.case();
        c.check(fact!(*case.result()).expect(fact!(case.input().value)));
        c.assert_certify();
        return;
    }
    for mode in ["quiet", "report", "panic", "invalid_seed"] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "planned_process_contract", "--nocapture"])
            .env(MODE, mode)
            .env(
                "SUN_CERT_SEED",
                if mode == "invalid_seed" {
                    "not a seed"
                } else {
                    "321"
                },
            )
            .env("SUN_CERT_CASES_BUDGET", "5")
            .env("SUN_CERT_REPORT", if mode == "report" { "1" } else { "0" })
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(
            output.status.success(),
            matches!(mode, "quiet" | "report"),
            "{stdout}
{stderr}"
        );
        match mode {
            "quiet" => {
                assert!(stderr.is_empty(), "{stderr}");
                assert!(!stdout.contains("CERTIFICATION"));
            }
            "report" => {
                assert!(stderr.contains("CERTIFICATION REPORT"));
                assert!(stderr.contains("seed: 321"));
                assert!(stderr.contains("cases executed: 5 / budget 5"));
                assert!(stderr.contains("coverage:"));
            }
            "panic" => {
                assert!(stderr.contains("failure budget reached (2/2)"));
                assert_eq!(stderr.matches("panicked at").count(), 1);
                assert!(stderr.len() < 10000);
            }
            _ => assert!(stderr.contains("invalid SUN_CERT_SEED")),
        }
    }
}
