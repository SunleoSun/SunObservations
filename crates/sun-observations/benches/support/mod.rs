use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
    process::Command,
};

const CHILD_ENV: &str = "SUN_OBS_INTERNAL_BENCHMARK_RUN_DIRECTORY";

// Capture outside the measured process so report I/O cannot enter benchmark timings or allocation counts.
pub fn run(benchmark: fn()) {
    if std::env::var_os(CHILD_ENV).is_some() {
        benchmark();
        return;
    }

    let directory = tempdir().expect("create benchmark run directory").keep();
    let metrics = directory.join("metrics.csv");
    let diagnostics = directory.join("stderr.txt");
    eprintln!("Benchmark reports: {}", directory.display());
    let status = Command::new(std::env::current_exe().expect("locate benchmark executable"))
        .args(std::env::args_os().skip(1))
        .env(CHILD_ENV, &directory)
        .stdout(File::create(&metrics).expect("create benchmark CSV"))
        .stderr(File::create(&diagnostics).expect("create benchmark diagnostics"))
        .status()
        .expect("run benchmark process");

    replay(&metrics, &mut io::stdout().lock());
    replay(&diagnostics, &mut io::stderr().lock());
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}

pub fn output_directory() -> io::Result<PathBuf> {
    let executable = std::env::current_exe()?;
    let deps = executable
        .parent()
        .ok_or_else(|| io::Error::other("benchmark executable has no parent"))?;
    let profile = deps
        .parent()
        .ok_or_else(|| io::Error::other("benchmark executable has no profile directory"))?;
    let target = profile
        .parent()
        .ok_or_else(|| io::Error::other("benchmark executable has no target directory"))?;
    Ok(target.join("benchmarks"))
}

pub fn tempdir() -> io::Result<tempfile::TempDir> {
    let directory = match std::env::var_os(CHILD_ENV) {
        Some(path) => PathBuf::from(path),
        None => output_directory()?,
    };
    std::fs::create_dir_all(&directory)?;
    tempfile::Builder::new()
        .prefix(concat!(env!("CARGO_CRATE_NAME"), "-"))
        .tempdir_in(directory)
}

fn replay(path: &Path, output: &mut impl io::Write) {
    io::copy(
        &mut File::open(path).expect("read benchmark report"),
        output,
    )
    .expect("replay benchmark report");
}
