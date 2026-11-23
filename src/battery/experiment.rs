//! Experiment directories: everything one battery experiment produces lives in
//! one directory under `results/`, next to the record of how it was made.
//!
//! ```text
//! results/<YYYYmmdd-HHMMSS>-battery[-<label>]/
//!   battery.json   results (`runner::BenchRun`)
//!   meta.json      one entry per command run against this experiment
//!   report.md      cross-library report; tables only until
//!                  `scripts/bench.py battery` fills its figure slots
//!   plots/         `scripts/bench.py battery` output, linked from the report
//! results/latest-battery -> the most recently created experiment
//! ```
//!
//! This follows `scripts/bench.py run`, which writes Criterion experiments to
//! `results/<YYYYmmdd-HHMMSS>[-<label>]/` with a `meta.json`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

/// File name of the results inside an experiment directory.
pub const RESULTS_FILE: &str = "battery.json";
const META_FILE: &str = "meta.json";
const LATEST: &str = "latest-battery";

/// The `results/` directory at the root of the repository.
pub fn results_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("results")
}

/// Path of an experiment's results file.
pub fn results_file(dir: &Path) -> PathBuf {
    dir.join(RESULTS_FILE)
}

/// Create a new, empty experiment directory and point `latest-battery` at it.
pub fn create(label: Option<&str>) -> Result<PathBuf> {
    let stamp = command_output("date", &["+%Y%m%d-%H%M%S"]).unwrap_or_else(|| {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        secs.to_string()
    });
    let name = match label {
        Some(label) => {
            if label.is_empty() || label.contains(['/', '\\']) || label.starts_with('.') {
                bail!("experiment label {label:?} must be a plain name");
            }
            format!("{stamp}-battery-{label}")
        }
        None => format!("{stamp}-battery"),
    };
    let dir = results_root().join(name);
    if dir.exists() {
        bail!("experiment directory {} already exists", dir.display());
    }
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    set_latest(&dir)?;
    Ok(dir)
}

/// The most recently created experiment.
pub fn latest() -> Result<PathBuf> {
    let link = results_root().join(LATEST);
    let target = std::fs::read_link(&link).with_context(|| {
        format!(
            "no latest experiment ({} is missing); run `battery run` first or pass --experiment",
            link.display()
        )
    })?;
    Ok(results_root().join(target))
}

fn set_latest(dir: &Path) -> Result<()> {
    let link = results_root().join(LATEST);
    if link.symlink_metadata().is_ok() {
        std::fs::remove_file(&link).with_context(|| format!("replacing {}", link.display()))?;
    }
    let name = dir.file_name().context("experiment directory has a name")?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(name, &link)
        .with_context(|| format!("linking {}", link.display()))?;
    #[cfg(not(unix))]
    std::fs::write(&link, name.to_string_lossy().as_bytes())
        .with_context(|| format!("writing {}", link.display()))?;
    Ok(())
}

/// Append a record of this command to the experiment's `meta.json`: the
/// command line, the code and toolchain it ran with, and the input data
/// files (size and SHA-256), so an experiment can be traced back to the
/// fixtures it used.
pub fn record_command(dir: &Path, data_dir: &Path) -> Result<()> {
    let path = dir.join(META_FILE);
    let mut meta: Value = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing {}", path.display()))?,
        Err(_) => json!({ "experiment": dir.file_name().map(|n| n.to_string_lossy()), "commands": [] }),
    };
    let entry = json!({
        "started": command_output("date", &["--iso-8601=seconds"]),
        "argv": std::env::args().collect::<Vec<_>>(),
        "git_commit": command_output("git", &["rev-parse", "--short", "HEAD"]),
        "git_dirty": command_output("git", &["status", "--porcelain", "--untracked-files=no"])
            .is_some_and(|s| !s.is_empty()),
        "rustc": command_output("rustc", &["--version"]),
        "crates": crate_versions(&["automerge", "yrs"]),
        "features": enabled_features(),
        "data_dir": data_dir.display().to_string(),
        "data": data_files(data_dir),
    });
    meta["commands"]
        .as_array_mut()
        .with_context(|| format!("{} has no commands list", path.display()))?
        .push(entry);
    std::fs::write(&path, serde_json::to_string_pretty(&meta)? + "\n")
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn enabled_features() -> Vec<&'static str> {
    let mut features = Vec::new();
    if cfg!(feature = "memory") {
        features.push("memory");
    }
    features
}

/// Versions of `names` from `Cargo.lock`.
fn crate_versions(names: &[&str]) -> Value {
    let lock = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock"))
        .unwrap_or_default();
    let mut versions = serde_json::Map::new();
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if let Some(name) = line.strip_prefix("name = \"").and_then(|s| s.strip_suffix('"'))
            && names.contains(&name)
            && let Some(version) = lines
                .next()
                .and_then(|l| l.strip_prefix("version = \""))
                .and_then(|s| s.strip_suffix('"'))
        {
            versions.insert(name.to_string(), json!(version));
        }
    }
    Value::Object(versions)
}

/// Size and SHA-256 of every file under `data_dir`, keyed by relative path.
fn data_files(data_dir: &Path) -> Value {
    let mut files = Vec::new();
    collect_files(data_dir, &mut files);
    files.sort();
    let mut out = serde_json::Map::new();
    for file in files {
        let Ok(relative) = file.strip_prefix(data_dir) else { continue };
        let size = std::fs::metadata(&file).map(|m| m.len()).ok();
        let sha256 = command_output("sha256sum", &[&file.to_string_lossy()])
            .and_then(|s| s.split_whitespace().next().map(str::to_string));
        out.insert(
            relative.display().to_string(),
            json!({ "bytes": size, "sha256": sha256 }),
        );
    }
    Value::Object(out)
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}
