use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

const USAGE: &str = "cargo sun-observations-skills sync [--skills-dir <PATH>]

Default: <consumer-workspace>/.agents/skills
Relative --skills-dir paths are relative to the invocation directory.";

/// The Cargo-resolved graph, not the installed tool's dependencies, owns source identity.
#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    resolve: Option<Resolve>,
    workspace_root: PathBuf,
}

#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
}

/// manifest_path points into Cargo's exact registry extraction, Git checkout or local package.
#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    manifest_path: PathBuf,
}

/// Fully validated source bytes, selected before any destination writes.
struct Skill {
    package_id: String,
    source: PathBuf,
    bytes: Vec<u8>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args: Vec<OsString> = env::args_os().skip(1).collect();
    if args
        .first()
        .is_some_and(|arg| arg == "sun-observations-skills")
    {
        args.remove(0);
    }
    if args.as_slice() == ["--help"] || args.as_slice() == ["-h"] {
        println!("{USAGE}");
        return Ok(());
    }
    if args.first().is_none_or(|arg| arg != "sync") {
        return Err(format!("expected sync\n{USAGE}"));
    }
    let custom = match &args[1..] {
        [] => None,
        [flag] if flag == "--help" || flag == "-h" => {
            println!("{USAGE}");
            return Ok(());
        }
        [flag, path] if flag == "--skills-dir" && !path.is_empty() => Some(PathBuf::from(path)),
        _ => return Err(format!("invalid sync options\n{USAGE}")),
    };
    let cwd = env::current_dir().map_err(|error| format!("read invocation directory: {error}"))?;
    let metadata = cargo_metadata()?;
    let skills = discover(metadata.packages, metadata.resolve)?;
    let base = custom.map_or_else(
        || metadata.workspace_root.join(".agents/skills"),
        |path| cwd.join(path),
    );
    sync(&base, &skills)?;
    for (name, skill) in skills {
        println!(
            "Synced {name} from {} ({}) -> {}",
            skill.package_id,
            skill.source.display(),
            base.join(name).join("SKILL.md").display()
        );
    }
    Ok(())
}

fn cargo_metadata() -> Result<Metadata, String> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--locked"])
        .output()
        .map_err(|error| format!("run cargo metadata in consumer directory: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata --locked failed ({}):\n{}\nensure the locked sources are available; if the graph needs resolution, run Cargo explicitly (for example cargo generate-lockfile), then retry sync",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid cargo metadata response: {error}"))
}

fn discover(
    packages: Vec<Package>,
    resolve: Option<Resolve>,
) -> Result<BTreeMap<&'static str, Skill>, String> {
    let resolved: BTreeSet<_> = resolve
        .ok_or("cargo metadata omitted the resolved graph")?
        .nodes
        .into_iter()
        .map(|node| node.id)
        .collect();
    let mut owners: BTreeMap<&str, Vec<Package>> = BTreeMap::new();
    for package in packages {
        if !resolved.contains(&package.id) {
            continue;
        }
        let name = match package.name.as_str() {
            "sun-certifications" => "certifications",
            "sun-observations" => "sun-observations",
            _ => continue,
        };
        owners.entry(name).or_default().push(package);
    }
    if owners.is_empty() {
        return Err("no resolved sun-observations or sun-certifications owner packages; run sync in the consumer Cargo workspace".into());
    }
    let mut skills = BTreeMap::new();
    for (name, mut packages) in owners {
        packages.sort_by(|left, right| left.id.cmp(&right.id));
        if packages.len() != 1 {
            return Err(format!(
                "ambiguous skill {name}: multiple resolved owner identities:\n{}\nuse one owner identity in the consumer graph before syncing",
                packages
                    .iter()
                    .map(|package| format!(
                        "  {} ({})",
                        package.id,
                        package.manifest_path.display()
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
        let package = packages.pop().unwrap();
        let directory = package
            .manifest_path
            .parent()
            .ok_or_else(|| format!("invalid manifest_path for {}", package.id))?;
        let source = directory.join("agent-skills").join(name).join("SKILL.md");
        let context = || format!("skill {name} from {} at {}", package.id, source.display());
        let canonical_directory = directory
            .canonicalize()
            .map_err(|error| format!("{}: {error}", context()))?;
        let canonical_source = source.canonicalize().map_err(|error| {
            format!(
                "missing canonical {}: {error}; the resolved owner package must contain its skill",
                context()
            )
        })?;
        if !canonical_source.starts_with(&canonical_directory) {
            return Err(format!(
                "{} escapes the resolved package directory",
                context()
            ));
        }
        let bytes =
            fs::read(&canonical_source).map_err(|error| format!("read {}: {error}", context()))?;
        validate_identity(&bytes, name).map_err(|error| format!("{}: {error}", context()))?;
        skills.insert(
            name,
            Skill {
                package_id: package.id,
                source,
                bytes,
            },
        );
    }
    Ok(skills)
}

fn validate_identity(bytes: &[u8], expected: &str) -> Result<(), String> {
    let text =
        std::str::from_utf8(bytes).map_err(|error| format!("skill is not UTF-8: {error}"))?;
    let mut lines = text.lines();
    if lines.next() != Some("---") {
        return Err("missing skill front matter".into());
    }
    let mut names = Vec::new();
    for line in lines {
        if line == "---" {
            return if names == [expected] {
                Ok(())
            } else {
                Err(format!(
                    "skill identity must be exactly name: {expected}; found {names:?}"
                ))
            };
        }
        if let Some(name) = line.strip_prefix("name:") {
            names.push(name.trim());
        }
    }
    Err("unterminated skill front matter".into())
}

fn validate_destination(path: &Path, directory: bool) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if !metadata.file_type().is_symlink()
                && if directory {
                    metadata.is_dir()
                } else {
                    metadata.is_file()
                } =>
        {
            Ok(())
        }
        Ok(_) => Err(format!(
            "unsafe destination {}: expected a non-symlink {}",
            path.display(),
            if directory { "directory" } else { "file" }
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("inspect destination {}: {error}", path.display())),
    }
}

fn sync(base: &Path, skills: &BTreeMap<&str, Skill>) -> Result<(), String> {
    for ancestor in base.ancestors() {
        validate_destination(ancestor, true)?;
    }
    for name in skills.keys() {
        let directory = base.join(name);
        validate_destination(&directory, true)?;
        validate_destination(&directory.join("SKILL.md"), false)?;
    }
    for (name, skill) in skills {
        let directory = base.join(name);
        fs::create_dir_all(&directory)
            .map_err(|error| format!("create {}: {error}", directory.display()))?;
        let destination = directory.join("SKILL.md");
        match fs::read(&destination) {
            Ok(bytes) if bytes == skill.bytes => continue,
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("read {}: {error}", destination.display())),
        }
        let mut temporary = tempfile::NamedTempFile::new_in(&directory).map_err(|error| {
            format!("create temporary file in {}: {error}", directory.display())
        })?;
        temporary
            .write_all(&skill.bytes)
            .map_err(|error| format!("write {}: {error}", destination.display()))?;
        temporary
            .persist(&destination)
            .map_err(|error| format!("replace {}: {error}", destination.display()))?;
    }
    Ok(())
}
