# Package-owned agent skills

Operational/tooling reference. For learning the framework, use the
[human guide](../README.md); for other documentation roles, see the [map](README.md).

Install the Cargo subcommand binary from the framework package (after release):

```text
cargo install sun-observations --bin cargo-sun-observations-skills
# From this checkout before release:
cargo install --path crates/sun-observations --bin cargo-sun-observations-skills
```

In the consumer Cargo workspace, run:

```text
cargo sun-observations-skills sync
cargo sun-observations-skills sync --skills-dir .my-agent/skills
```

Default output is `<consumer-workspace>/.agents/skills/<name>/SKILL.md`.
A relative `--skills-dir` is relative to the invocation directory; an absolute
path is used directly. The custom layout is the same as the default. Only the
discovered owner skills are updated; unrelated skills are not deleted. Identical
files are left untouched, and stale files are replaced byte-for-byte via a
temporary file in the destination directory and an atomic replacement, without
truncating files shared through hard links.

## Canonical ownership

- [certifications](../crates/sun-observations/agent-skills/certifications/SKILL.md)
  belongs to the `sun-observations` Cargo package.
- [sun-observations](../crates/sun-observations/agent-skills/sun-observations/SKILL.md)
  belongs to the same `sun-observations` Cargo package. Capture/query/decode/
  provenance remains owned by its evidence layer, not generic certification.

The framework manifest explicitly includes `agent-skills/**` in their Cargo package
contents. The delivery tool does not embed manuals or depend on its own version
to select them. No ordinary build triggers synchronization.

## Exact resolved sources

The tool runs `cargo metadata --format-version 1 --locked` in the consumer invocation
directory and uses the active resolved package IDs and their `manifest_path`.
Inactive optional dependencies are not promoted into the graph merely for skill
discovery. Cargo handles lockfiles, semver, transitive dependencies, patches,
registry sources, and pinned Git revisions. The consumer graph must already be
resolved: if its lockfile is absent or needs updating, run Cargo resolution
explicitly (for example `cargo generate-lockfile`) before sync. Sync never creates
or updates the consumer lockfile. Metadata may need Cargo registry/Git access on a
cold source cache; there is no independent skill download. Cargo offline
configuration is respected.

The canonical file is read from
`<resolved-package-directory>/agent-skills/<name>/SKILL.md`. For registry sources
this is the resolved version extraction; for Git it is Cargo's resolved revision
checkout; for path/workspace sources it is that local package directory. The
front matter must have exactly one `name:` matching the owned skill layout.
A canonical source cannot escape its package directory through a symlink.

No resolved sun-observations owner, either missing/invalid canonical skill, or multiple
active resolved sun-observations identities cause a nonzero exit with diagnostics before any skill
writes. Multiple versions are rejected even when their content is identical.
The tool never chooses newest, direct, or installed-tool versions to break ties.
Destination symlinks and non-file/non-directory collisions are rejected. An I/O
failure is reported as a failure; synchronization is not a multi-file transaction.

## Repository self-sync

Edit only the package-local canonical skills. Regenerate the checked root mirrors
from the workspace root with the installed command above, or without installation:

```text
cargo run -p sun-observations --bin cargo-sun-observations-skills -- sync
```

Documentation certifications read canonical sources for API/methodology/example
checks and enforce exact equality with `.agents/skills/<name>/SKILL.md`. A stale
mirror is a hard violation; there is no two-way synchronization. Integration tests
exercise installed Cargo dispatch, exact path/Git/registry-directory sources,
updates, custom destinations, failures, and owner `cargo package --list` contents.
