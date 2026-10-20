use std::process::Command;

#[test]
fn stdout_stderr_and_local_drop_or_global_flush_survive_process_exit() {
    let expected: String = (0..4000)
        .map(|index| format!("{index}\ncontinued\n"))
        .collect();
    for mode in ["stdout-drop", "stderr-drop", "stdout-flush", "stderr-flush"] {
        let result = Command::new(env!("CARGO_BIN_EXE_console_scenario"))
            .arg(mode)
            .output()
            .unwrap();
        assert!(result.status.success(), "{mode}: {:?}", result.status);
        let (human, unused) = if mode.starts_with("stdout") {
            (&result.stdout, &result.stderr)
        } else {
            (&result.stderr, &result.stdout)
        };
        assert!(unused.is_empty());
        assert_eq!(human, expected.as_bytes());
    }
}
