use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::TempDir;

fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn skill(name: &str, marker: &str) -> String {
    format!("---\nname: {name}\ndescription: Exact-source fixture\n---\n{marker}\n")
}

fn owner(directory: &Path, package: &str, version: &str, name: &str, marker: &str) -> PathBuf {
    write(
        directory.join("Cargo.toml"),
        format!(
            r#"[package]
name={package:?}
version={version:?}
edition="2021"
[workspace]
"#
        ),
    );
    write(directory.join("src/lib.rs"), "");
    for skill_name in ["sun-observations", "certifications"] {
        write(
            directory
                .join("agent-skills")
                .join(skill_name)
                .join("SKILL.md"),
            skill(skill_name, marker),
        );
    }
    let source = directory.join("agent-skills").join(name).join("SKILL.md");
    write(&source, skill(name, marker));
    source
}

fn consumer(directory: &Path, dependencies: &str) {
    write(
        directory.join("Cargo.toml"),
        format!(
            r#"[package]
name="consumer"
version="0.0.0"
edition="2021"
[workspace]
[dependencies]
{dependencies}
"#
        ),
    );
    write(directory.join("src/lib.rs"), "");
}

fn path(path: &Path) -> String {
    format!("{:?}", path.to_str().unwrap().replace(char::from(92), "/"))
}

fn checked(output: Output) -> Output {
    assert!(
        output.status.success(),
        "status: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn cargo(cwd: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO"))
        .current_dir(cwd)
        .args(arguments)
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .unwrap()
}

fn sync_with_binary(cwd: &Path, bin_directory: &Path, arguments: &[&str]) -> Output {
    let mut paths = vec![bin_directory.to_owned()];
    paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    Command::new(env!("CARGO"))
        .current_dir(cwd)
        .args(["sun-observations-skills", "sync"])
        .args(arguments)
        .env("PATH", env::join_paths(paths).unwrap())
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .unwrap()
}

fn sync(cwd: &Path, arguments: &[&str]) -> Output {
    sync_with_binary(
        cwd,
        Path::new(env!("CARGO_BIN_EXE_cargo-sun-observations-skills"))
            .parent()
            .unwrap(),
        arguments,
    )
}

fn rejected(output: Output, diagnostic: &str) -> String {
    assert!(
        !output.status.success(),
        "unexpected success: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains(diagnostic), "{stderr}");
    stderr
}

fn metadata(directory: &Path) -> serde_json::Value {
    serde_json::from_slice(
        &checked(cargo(directory, &["metadata", "--format-version", "1"])).stdout,
    )
    .unwrap()
}

#[test]
fn external_subcommand_syncs_transitive_exact_path_sources_and_updates_only_selected_destination() {
    let temporary = TempDir::new().unwrap();
    let root = temporary.path();
    let source = owner(
        &root.join("resolved-a"),
        "sun-observations",
        "3.2.1",
        "certifications",
        "resolved A",
    );
    owner(
        &root.join("unresolved-b"),
        "sun-observations",
        "9.8.7",
        "certifications",
        "wrong B",
    );
    let runtime = root.join("resolved-a/agent-skills/sun-observations/SKILL.md");
    let bridge = root.join("bridge");
    write(
        bridge.join("Cargo.toml"),
        format!(
            r#"[package]
name="bridge"
version="0.0.0"
[workspace]
[dependencies]
sun-observations={{path={}}}
"#,
            path(&root.join("resolved-a"))
        ),
    );
    write(bridge.join("src/lib.rs"), "");
    let consumer_root = root.join("app");
    consumer(
        &consumer_root,
        &format!("bridge={{path={}}}", path(&bridge)),
    );
    metadata(&consumer_root);
    let lockfile = fs::read(consumer_root.join("Cargo.lock")).unwrap();
    checked(sync(&consumer_root, &[]));
    assert_eq!(
        fs::read(consumer_root.join("Cargo.lock")).unwrap(),
        lockfile
    );
    let destination = consumer_root.join(".agents/skills");
    let mirror = destination.join("certifications/SKILL.md");
    assert_eq!(fs::read(&mirror).unwrap(), fs::read(&source).unwrap());
    assert_eq!(
        fs::read(destination.join("sun-observations/SKILL.md")).unwrap(),
        fs::read(&runtime).unwrap()
    );
    let modified = fs::metadata(&mirror).unwrap().modified().unwrap();
    checked(sync(&consumer_root, &[]));
    assert_eq!(fs::metadata(&mirror).unwrap().modified().unwrap(), modified);
    write(destination.join("unrelated/SKILL.md"), "keep me");
    write(&mirror, "stale mirror");
    checked(sync(&consumer_root, &[]));
    assert_eq!(fs::read(&mirror).unwrap(), fs::read(&source).unwrap());
    write(&source, skill("certifications", "canonical edit"));
    checked(sync(&consumer_root, &[]));
    assert_eq!(fs::read(&mirror).unwrap(), fs::read(&source).unwrap());
    assert_eq!(
        fs::read(destination.join("unrelated/SKILL.md")).unwrap(),
        b"keep me"
    );
    let nested = consumer_root.join("src");
    write(&mirror, "must not update during custom sync");
    checked(sync(&nested, &["--skills-dir", ".my-agent/skills"]));
    assert_eq!(
        fs::read(nested.join(".my-agent/skills/certifications/SKILL.md")).unwrap(),
        fs::read(&source).unwrap()
    );
    assert!(!nested.join(".agents").exists());
    assert_eq!(
        fs::read(&mirror).unwrap(),
        b"must not update during custom sync"
    );
    checked(sync(&nested, &[]));
    assert_eq!(fs::read(&mirror).unwrap(), fs::read(&source).unwrap());
    let names: std::collections::BTreeSet<_> = fs::read_dir(&destination)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(
        names,
        ["certifications", "sun-observations", "unrelated"]
            .into_iter()
            .map(Into::into)
            .collect()
    );
}

#[test]
fn distinct_resolved_versions_fail_before_any_skill_is_written() {
    let temporary = TempDir::new().unwrap();
    let root = temporary.path();
    owner(
        &root.join("a"),
        "sun-observations",
        "1.0.0",
        "certifications",
        "A",
    );
    owner(
        &root.join("b"),
        "sun-observations",
        "2.0.0",
        "certifications",
        "B",
    );
    let app = root.join("app");
    consumer(
        &app,
        &format!(
            r#"a={{package="sun-observations",path={}}}
b={{package="sun-observations",path={}}}"#,
            path(&root.join("a")),
            path(&root.join("b"))
        ),
    );
    metadata(&app);
    let diagnostic = rejected(sync(&app, &[]), "ambiguous skill sun-observations");
    assert!(
        diagnostic.contains("1.0.0") && diagnostic.contains("2.0.0"),
        "{diagnostic}"
    );
    assert!(!app.join(".agents").exists());
}

#[test]
fn inactive_optional_owner_version_does_not_conflict_with_the_active_resolved_api() {
    let temporary = TempDir::new().unwrap();
    let root = temporary.path();
    let source = owner(
        &root.join("active"),
        "sun-observations",
        "1.0.0",
        "certifications",
        "active API",
    );
    owner(
        &root.join("optional"),
        "sun-observations",
        "2.0.0",
        "certifications",
        "inactive optional API",
    );
    let app = root.join("app");
    consumer(
        &app,
        &format!(
            r#"active={{package="sun-observations",path={}}}
optional={{package="sun-observations",path={},optional=true}}"#,
            path(&root.join("active")),
            path(&root.join("optional"))
        ),
    );
    metadata(&app);
    checked(sync(&app, &[]));
    assert_eq!(
        fs::read(app.join(".agents/skills/certifications/SKILL.md")).unwrap(),
        fs::read(source).unwrap()
    );
}

#[test]
fn missing_or_invalid_canonical_sources_fail_without_overwriting_existing_mirrors() {
    for bytes in [
        None,
        Some("---\nname: other\n---\n"),
        Some("---\nname: certifications\nname: certifications\n---\n"),
        Some("---\nname: certifications\n"),
        Some("not front matter"),
    ] {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path();
        let source = owner(
            &root.join("owner"),
            "sun-observations",
            "7.0.0",
            "certifications",
            "A",
        );
        if let Some(bytes) = bytes {
            write(&source, bytes);
        } else {
            fs::remove_file(&source).unwrap();
        }
        let app = root.join("app");
        consumer(
            &app,
            &format!("sun-observations={{path={}}}", path(&root.join("owner"))),
        );
        let mirror = app.join(".agents/skills/certifications/SKILL.md");
        write(&mirror, "preserve on failure");
        metadata(&app);
        let diagnostic = rejected(sync(&app, &[]), "skill certifications");
        assert!(diagnostic.contains("7.0.0"), "{diagnostic}");
        assert_eq!(fs::read(&mirror).unwrap(), b"preserve on failure");
        assert!(!app.join(".agents/skills/sun-observations").exists());
    }
}

#[test]
fn registry_directory_source_uses_resolved_version_not_an_adjacent_newer_package() {
    let temporary = TempDir::new().unwrap();
    let root = temporary.path();
    let registry = root.join("registry");
    let source = owner(
        &registry.join("sun-observations-1.2.3"),
        "sun-observations",
        "1.2.3",
        "certifications",
        "registry resolved A",
    );
    owner(
        &registry.join("sun-observations-2.0.0"),
        "sun-observations",
        "2.0.0",
        "certifications",
        "registry wrong B",
    );
    for version in ["1.2.3", "2.0.0"] {
        write(
            registry.join(format!("sun-observations-{version}/.cargo-checksum.json")),
            r#"{"files":{},"package":null}"#,
        );
    }
    let app = root.join("app");
    consumer(&app, "sun-observations=\"=1.2.3\"");
    write(
        app.join(".cargo/config.toml"),
        format!(
            "[source.crates-io]\nreplace-with=\"fixture\"\n[source.fixture]\ndirectory={}\n",
            path(&registry)
        ),
    );
    let graph = metadata(&app);
    let resolved = graph["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| package["name"] == "sun-observations")
        .unwrap();
    assert!(resolved["source"]
        .as_str()
        .unwrap()
        .starts_with("registry+"));
    assert_eq!(
        Path::new(resolved["manifest_path"].as_str().unwrap())
            .parent()
            .unwrap(),
        source.ancestors().nth(3).unwrap()
    );
    let locked = fs::read(app.join("Cargo.lock")).unwrap();
    consumer(&app, "sun-observations=\">=1,<3\"");
    checked(sync(&app, &[]));
    assert_eq!(fs::read(app.join("Cargo.lock")).unwrap(), locked);
    assert_eq!(
        fs::read(app.join(".agents/skills/certifications/SKILL.md")).unwrap(),
        fs::read(source).unwrap()
    );
}

fn git(directory: &Path, arguments: &[&str]) -> String {
    String::from_utf8(
        checked(
            Command::new("git")
                .current_dir(directory)
                .args([
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(arguments)
                .output()
                .unwrap(),
        )
        .stdout,
    )
    .unwrap()
    .trim()
    .to_owned()
}

#[test]
fn git_revision_is_read_from_cargos_checkout_not_repository_head() {
    let temporary = TempDir::new().unwrap();
    let root = temporary.path();
    let repository = root.join("repository");
    let source = owner(
        &repository,
        "sun-observations",
        "5.0.0",
        "sun-observations",
        "revision A",
    );
    git(&repository, &["init"]);
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "-m", "revision A"]);
    let revision_a = git(&repository, &["rev-parse", "HEAD"]);
    write(&source, skill("sun-observations", "wrong revision B"));
    write(
        repository.join("agent-skills/certifications/SKILL.md"),
        skill("certifications", "wrong revision B"),
    );
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "-m", "revision B"]);
    let revision_b = git(&repository, &["rev-parse", "HEAD"]);
    assert_ne!(revision_a, revision_b);
    let repository_path = repository.to_str().unwrap().replace(char::from(92), "/");
    let url = if repository_path.starts_with('/') {
        format!("file://{repository_path}")
    } else {
        format!("file:///{repository_path}")
    };
    let app = root.join("app");
    consumer(
        &app,
        &format!("sun-observations={{git={url:?},rev={revision_a:?}}}"),
    );
    let output = checked(
        Command::new(env!("CARGO"))
            .current_dir(&app)
            .args(["metadata", "--format-version", "1"])
            .env("CARGO_NET_OFFLINE", "false")
            .output()
            .unwrap(),
    );
    let graph: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let resolved = graph["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| package["name"] == "sun-observations")
        .unwrap();
    assert!(resolved["source"].as_str().unwrap().ends_with(&revision_a));
    let checkout = Path::new(resolved["manifest_path"].as_str().unwrap())
        .parent()
        .unwrap();
    assert_ne!(checkout, repository);
    metadata(&app);
    checked(sync(&app, &[]));
    let mirror = app.join(".agents/skills/sun-observations/SKILL.md");
    assert_eq!(
        fs::read(&mirror).unwrap(),
        fs::read(checkout.join("agent-skills/sun-observations/SKILL.md")).unwrap()
    );
    assert_eq!(
        fs::read(&mirror).unwrap(),
        skill("sun-observations", "revision A").as_bytes()
    );
    assert_ne!(fs::read(&mirror).unwrap(), fs::read(source).unwrap());
    assert_eq!(
        fs::read(app.join(".agents/skills/certifications/SKILL.md")).unwrap(),
        skill("certifications", "revision A").as_bytes()
    );
    consumer(&app, &format!(
        "sun-observations={{git={url:?},rev={revision_a:?}}}\nother={{package=\"sun-observations\",path={}}}", path(&repository)
    ));
    metadata(&app);
    let diagnostic = rejected(sync(&app, &[]), "ambiguous skill sun-observations");
    assert!(diagnostic.contains(&revision_a), "{diagnostic}");
    assert_eq!(
        fs::read(&mirror).unwrap(),
        skill("sun-observations", "revision A").as_bytes()
    );
}

#[test]
fn patches_are_resolved_by_cargo_before_selecting_skill_source() {
    let temporary = TempDir::new().unwrap();
    let root = temporary.path();
    let source = owner(
        &root.join("patched"),
        "sun-observations",
        "1.0.0",
        "certifications",
        "patch A",
    );
    let app = root.join("app");
    consumer(
        &app,
        &format!(
            "sun-observations=\"=1.0.0\"\n[patch.crates-io]\nsun-observations={{path={}}}",
            path(&root.join("patched"))
        ),
    );
    metadata(&app);
    checked(sync(&app, &[]));
    assert_eq!(
        fs::read(app.join(".agents/skills/certifications/SKILL.md")).unwrap(),
        fs::read(source).unwrap()
    );
}

#[test]
fn installation_is_self_contained_and_cargo_invokes_the_installed_external_subcommand() {
    let temporary = TempDir::new().unwrap();
    let root = temporary.path();
    let install = root.join("installed");
    let tool = Path::new(env!("CARGO_MANIFEST_DIR"));
    checked(cargo(
        root,
        &[
            "install",
            "--path",
            tool.to_str().unwrap(),
            "--root",
            install.to_str().unwrap(),
            "--offline",
            "--debug",
            "--target-dir",
            root.join("target").to_str().unwrap(),
        ],
    ));
    let source = owner(
        &root.join("owner"),
        "sun-observations",
        "8.0.0",
        "certifications",
        "installed tool must use consumer owner",
    );
    let app = root.join("app");
    consumer(
        &app,
        &format!("sun-observations={{path={}}}", path(&root.join("owner"))),
    );
    metadata(&app);
    checked(sync_with_binary(
        &app,
        &install.join("bin"),
        &["--skills-dir", "custom"],
    ));
    assert_eq!(
        fs::read(app.join("custom/certifications/SKILL.md")).unwrap(),
        fs::read(source).unwrap()
    );
    assert!(!app.join(".agents").exists());
}

#[test]
fn framework_package_contains_both_skills_and_all_executable_examples_and_benchmarks() {
    let package_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = checked(cargo(
        package_root,
        &["package", "--list", "--allow-dirty", "--offline"],
    ));
    let files: std::collections::BTreeSet<_> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|file| file.replace(char::from(92), "/"))
        .collect();
    for required in [
        "README.md",
        "src/lib.rs",
        "src/bin/cargo-sun-observations-skills.rs",
        "agent-skills/sun-observations/SKILL.md",
        "agent-skills/certifications/SKILL.md",
    ] {
        assert!(files.contains(required), "missing {required}: {files:?}");
    }
    for example in [
        "basic",
        "certification",
        "channels",
        "human",
        "jsonl",
        "module_relation",
        "rayon",
        "scoped",
        "skill_workflows",
        "spans",
        "threads",
    ] {
        assert!(
            files.contains(&format!("examples/{example}.rs")),
            "missing example {example}"
        );
    }
    for benchmark in [
        "runtime",
        "capture",
        "certifications",
        "analysis",
        "batch",
        "human",
        "investigation",
        "investigation_allocations",
        "support/mod",
        "support/report_process",
    ] {
        assert!(
            files.contains(&format!("benches/{benchmark}.rs")),
            "missing benchmark {benchmark}"
        );
    }
}

#[test]
fn cli_and_metadata_errors_are_nonzero_and_actionable() {
    let temporary = TempDir::new().unwrap();
    rejected(
        sync(temporary.path(), &["--unknown"]),
        "invalid sync options",
    );
    rejected(
        sync(temporary.path(), &["--skills-dir"]),
        "--skills-dir <PATH>",
    );
    rejected(
        sync(temporary.path(), &[]),
        "cargo metadata --locked failed",
    );
    consumer(temporary.path(), "");
    metadata(temporary.path());
    rejected(sync(temporary.path(), &[]), "no resolved");
    assert!(!temporary.path().join(".agents").exists());
}

#[test]
fn workspace_member_invocation_defaults_to_workspace_root() {
    let temporary = TempDir::new().unwrap();
    let root = temporary.path();
    let owner_directory = root.join("owner");
    let source = owner(
        &owner_directory,
        "sun-observations",
        "1.0.0",
        "certifications",
        "workspace owner",
    );
    write(
        owner_directory.join("Cargo.toml"),
        "[package]\nname=\"sun-observations\"\nversion=\"1.0.0\"\nedition=\"2021\"\n",
    );
    write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers=[\"app\",\"owner\"]\nresolver=\"2\"\n",
    );
    let app = root.join("app");
    write(app.join("Cargo.toml"), "[package]\nname=\"app\"\nversion=\"0.0.0\"\n[dependencies]\nsun-observations={path=\"../owner\"}\n");
    write(app.join("src/lib.rs"), "");
    metadata(&app);
    checked(sync(&app, &[]));
    assert_eq!(
        fs::read(root.join(".agents/skills/certifications/SKILL.md")).unwrap(),
        fs::read(&source).unwrap()
    );
    assert!(!app.join(".agents").exists());
    let custom = root.join("absolute custom/skills");
    checked(sync(&app, &["--skills-dir", custom.to_str().unwrap()]));
    assert_eq!(
        fs::read(custom.join("certifications/SKILL.md")).unwrap(),
        fs::read(source).unwrap()
    );
}

#[test]
fn locked_graph_is_required_and_sync_never_creates_or_updates_consumer_lockfiles() {
    let temporary = TempDir::new().unwrap();
    let root = temporary.path();
    let source = owner(
        &root.join("a"),
        "sun-observations",
        "1.0.0",
        "certifications",
        "A",
    );
    owner(
        &root.join("b"),
        "sun-observations",
        "2.0.0",
        "certifications",
        "B",
    );
    let app = root.join("app");
    consumer(
        &app,
        &format!("sun-observations={{path={}}}", path(&root.join("a"))),
    );
    let lock_path = app.join("Cargo.lock");
    rejected(
        sync(&app, &["--skills-dir", "custom"]),
        "cargo generate-lockfile",
    );
    assert!(!lock_path.exists());
    assert!(!app.join("custom").exists());
    metadata(&app);
    let lock_bytes = fs::read(&lock_path).unwrap();
    checked(sync(&app, &["--skills-dir", "custom"]));
    assert_eq!(fs::read(&lock_path).unwrap(), lock_bytes);
    let mirror = app.join("custom/certifications/SKILL.md");
    assert_eq!(fs::read(&mirror).unwrap(), fs::read(&source).unwrap());
    consumer(
        &app,
        &format!("sun-observations={{path={}}}", path(&root.join("b"))),
    );
    rejected(
        sync(&app, &["--skills-dir", "custom"]),
        "cargo metadata --locked failed",
    );
    assert_eq!(fs::read(&lock_path).unwrap(), lock_bytes);
    assert_eq!(fs::read(&mirror).unwrap(), fs::read(source).unwrap());
    assert!(!app.join(".agents").exists());
}

#[test]
fn replacing_a_stale_hard_link_does_not_modify_files_outside_destination() {
    let temporary = TempDir::new().unwrap();
    let root = temporary.path();
    let source = owner(
        &root.join("owner"),
        "sun-observations",
        "1.0.0",
        "certifications",
        "A",
    );
    let app = root.join("app");
    consumer(
        &app,
        &format!("sun-observations={{path={}}}", path(&root.join("owner"))),
    );
    let unrelated = root.join("unrelated.md");
    write(&unrelated, "do not change");
    let mirror = app.join(".agents/skills/certifications/SKILL.md");
    fs::create_dir_all(mirror.parent().unwrap()).unwrap();
    fs::hard_link(&unrelated, &mirror).unwrap();
    metadata(&app);
    checked(sync(&app, &[]));
    assert_eq!(fs::read(&mirror).unwrap(), fs::read(source).unwrap());
    assert_eq!(fs::read(unrelated).unwrap(), b"do not change");
    assert_eq!(fs::read_dir(mirror.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn destination_collisions_fail_without_writing_other_skills() {
    let temporary = TempDir::new().unwrap();
    let root = temporary.path();
    owner(
        &root.join("certifications"),
        "sun-observations",
        "1.0.0",
        "certifications",
        "A",
    );
    let app = root.join("app");
    consumer(
        &app,
        &format!(
            "sun-observations={{path={}}}",
            path(&root.join("certifications"))
        ),
    );
    let base = app.join(".agents/skills");
    fs::create_dir_all(base.join("sun-observations/SKILL.md")).unwrap();
    metadata(&app);
    rejected(sync(&app, &[]), "unsafe destination");
    assert!(!base.join("certifications").exists());
}

#[cfg(unix)]
#[test]
fn symlinked_destinations_and_sources_cannot_escape_the_selected_boundaries() {
    use std::os::unix::fs::symlink;
    let temporary = TempDir::new().unwrap();
    let root = temporary.path();
    let source = owner(
        &root.join("owner"),
        "sun-observations",
        "1.0.0",
        "certifications",
        "A",
    );
    let app = root.join("app");
    consumer(
        &app,
        &format!("sun-observations={{path={}}}", path(&root.join("owner"))),
    );
    let outside = root.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::create_dir_all(app.join(".agents/skills")).unwrap();
    symlink(&outside, app.join(".agents/skills/certifications")).unwrap();
    metadata(&app);
    rejected(sync(&app, &[]), "unsafe destination");
    assert!(!outside.join("SKILL.md").exists());
    fs::remove_file(app.join(".agents/skills/certifications")).unwrap();
    fs::remove_file(&source).unwrap();
    write(
        outside.join("SKILL.md"),
        skill("certifications", "outside package"),
    );
    symlink(outside.join("SKILL.md"), &source).unwrap();
    metadata(&app);
    rejected(sync(&app, &[]), "escapes the resolved package");
}
