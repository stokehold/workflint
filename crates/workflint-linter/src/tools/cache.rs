//! The opt-in on-disk cache of shellcheck's findings (ADR 0012).
//!
//! Each entry is a file named by the SHA-256 hash of everything that decides shellcheck's
//! output: its version, its arguments, the text of the rcfile, `SHELLCHECK_OPTS`, and the
//! script. An entry holds the findings of one script. Only findings are stored: a script that
//! shellcheck could not check runs again next time. An entry that cannot be read or decoded is a
//! miss, and the run writes it again.

use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use workflint_rules::tools::ShellcheckFinding;

/// The directory of the cache of workflint: `$XDG_CACHE_HOME/workflint` when `XDG_CACHE_HOME`
/// is an absolute path, and otherwise the platform's cache directory: `~/Library/Caches` on
/// macOS, `%LOCALAPPDATA%` on Windows, and `~/.cache` elsewhere. `None` when the variable that
/// names the platform's directory is not set.
#[must_use]
pub fn default_cache_dir() -> Option<PathBuf> {
    cache_dir_from(|name| std::env::var_os(name))
}

/// [`default_cache_dir`] with `var` for the environment.
fn cache_dir_from(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let absolute = |name: &str| {
        var(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let base = absolute("XDG_CACHE_HOME").or_else(|| {
        if cfg!(windows) {
            absolute("LOCALAPPDATA")
        } else if cfg!(target_os = "macos") {
            absolute("HOME").map(|home| home.join("Library").join("Caches"))
        } else {
            absolute("HOME").map(|home| home.join(".cache"))
        }
    })?;
    Some(base.join("workflint"))
}

/// The entries of shellcheck in a directory.
#[derive(Debug)]
pub(crate) struct DiskCache {
    dir: PathBuf,
}

/// What an entry holds.
#[derive(Serialize, Deserialize)]
struct Entry {
    findings: Vec<Finding>,
}

/// A finding, as an entry stores it.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Finding {
    line: i64,
    column: i64,
    end_line: Option<i64>,
    end_column: Option<i64>,
    level: String,
    code: i64,
    message: String,
}

/// Makes the names of the files that entries are written to before they are renamed unique in
/// the process.
static WRITES: AtomicU64 = AtomicU64::new(0);

/// What a cache key is made of.
pub(crate) struct KeyParts<'a> {
    /// The output of `shellcheck --version`.
    pub(crate) version: &'a str,
    /// Every argument of shellcheck but the script: those of the command, then those of the run.
    pub(crate) args: &'a [&'a OsStr],
    /// The text of the rcfile, empty without one.
    pub(crate) rcfile: &'a [u8],
    /// `SHELLCHECK_OPTS`, if it is set.
    pub(crate) env_options: Option<&'a OsStr>,
    /// The script.
    pub(crate) script: &'a str,
}

impl KeyParts<'_> {
    /// The key: the SHA-256 hash of the parts, in hexadecimal. Each part is preceded by its
    /// length, so two different lists of parts never give the same bytes to hash.
    pub(crate) fn key(&self) -> String {
        let mut hasher = Sha256::new();
        let mut part = |bytes: &[u8]| {
            hasher.update((bytes.len() as u64).to_le_bytes());
            hasher.update(bytes);
        };
        part(b"workflint shellcheck cache 1");
        part(self.version.as_bytes());
        part(&(self.args.len() as u64).to_le_bytes());
        for arg in self.args {
            part(arg.as_encoded_bytes());
        }
        part(self.rcfile);
        match self.env_options {
            Some(options) => {
                part(b"1");
                part(options.as_encoded_bytes());
            }
            None => part(b"0"),
        }
        part(self.script.as_bytes());
        hasher
            .finalize()
            .iter()
            .fold(String::with_capacity(64), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            })
    }
}

impl DiskCache {
    /// The cache in `dir`, which is created when the first entry is written.
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
    }

    /// The findings stored under `key`, or `None` if there is no entry or it cannot be read.
    pub(crate) fn read(&self, key: &str) -> Option<Vec<ShellcheckFinding>> {
        let path = self.path(key);
        let bytes = fs::read(&path).ok()?;
        match serde_json::from_slice::<Entry>(&bytes) {
            Ok(entry) => Some(
                entry
                    .findings
                    .into_iter()
                    .map(|finding| ShellcheckFinding {
                        line: finding.line,
                        column: finding.column,
                        end_line: finding.end_line,
                        end_column: finding.end_column,
                        level: finding.level,
                        code: finding.code,
                        message: finding.message,
                    })
                    .collect(),
            ),
            Err(err) => {
                tracing::debug!(
                    "Ignoring the shellcheck cache entry {}: {err}",
                    path.display()
                );
                None
            }
        }
    }

    /// Stores `findings` under `key`. The entry is written to another file first and then
    /// renamed, so that a run that reads it at the same time sees the whole entry or none. A
    /// failure only logs, because the run goes on without the entry.
    pub(crate) fn write(&self, key: &str, findings: &[ShellcheckFinding]) {
        let entry = Entry {
            findings: findings
                .iter()
                .map(|finding| Finding {
                    line: finding.line,
                    column: finding.column,
                    end_line: finding.end_line,
                    end_column: finding.end_column,
                    level: finding.level.clone(),
                    code: finding.code,
                    message: finding.message.clone(),
                })
                .collect(),
        };
        let path = self.path(key);
        let partial = self.dir.join(format!(
            "{key}.{}.{}.partial",
            std::process::id(),
            WRITES.fetch_add(1, Ordering::Relaxed)
        ));
        let written = serde_json::to_vec(&entry)
            .map_err(std::io::Error::other)
            .and_then(|bytes| {
                fs::create_dir_all(&self.dir)?;
                fs::write(&partial, bytes)?;
                fs::rename(&partial, &path)
            });
        if let Err(err) = written {
            let _ = fs::remove_file(&partial);
            tracing::debug!(
                "Could not write the shellcheck cache entry {}: {err}",
                path.display()
            );
        }
    }

    /// The directory of the entries.
    #[cfg(test)]
    pub(crate) fn dir(&self) -> &std::path::Path {
        &self.dir
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[cfg(unix)]
    #[test]
    fn an_absolute_xdg_cache_home_is_the_base_of_the_cache() {
        assert_eq!(
            cache_dir_from(env(&[("XDG_CACHE_HOME", "/xdg"), ("HOME", "/home/u")])),
            Some(PathBuf::from("/xdg/workflint"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn without_an_absolute_xdg_cache_home_the_platform_directory_is_the_base() {
        let platform = if cfg!(target_os = "macos") {
            "/home/u/Library/Caches/workflint"
        } else {
            "/home/u/.cache/workflint"
        };
        for vars in [
            &[("HOME", "/home/u")][..],
            &[("XDG_CACHE_HOME", ""), ("HOME", "/home/u")],
            &[("XDG_CACHE_HOME", "relative"), ("HOME", "/home/u")],
        ] {
            assert_eq!(
                cache_dir_from(env(vars)),
                Some(PathBuf::from(platform)),
                "{vars:?}"
            );
        }
        assert_eq!(cache_dir_from(env(&[])), None);
    }

    #[cfg(windows)]
    #[test]
    fn on_windows_the_base_is_local_app_data_unless_xdg_cache_home_is_set() {
        assert_eq!(
            cache_dir_from(env(&[("LOCALAPPDATA", r"C:\Users\u\AppData\Local")])),
            Some(PathBuf::from(r"C:\Users\u\AppData\Local\workflint"))
        );
        assert_eq!(
            cache_dir_from(env(&[
                ("XDG_CACHE_HOME", r"D:\cache"),
                ("LOCALAPPDATA", r"C:\Local")
            ])),
            Some(PathBuf::from(r"D:\cache\workflint"))
        );
    }

    fn parts<'a>(args: &'a [&'a OsStr], script: &'a str) -> KeyParts<'a> {
        KeyParts {
            version: "version: 0.11.0",
            args,
            rcfile: b"",
            env_options: None,
            script,
        }
    }

    #[test]
    fn every_part_of_the_key_changes_it() {
        let args = [OsStr::new("--norc"), OsStr::new("-x")];
        let base = parts(&args, "echo").key();
        assert_eq!(base.len(), 64);
        assert_eq!(parts(&args, "echo").key(), base);
        let fewer = [OsStr::new("--norc")];
        let joined = [OsStr::new("--norc-x")];
        let changed = [
            KeyParts {
                version: "version: 0.10.0",
                ..parts(&args, "echo")
            },
            parts(&fewer, "echo"),
            parts(&joined, "echo"),
            KeyParts {
                rcfile: b"disable=SC2086",
                ..parts(&args, "echo")
            },
            KeyParts {
                env_options: Some(OsStr::new("")),
                ..parts(&args, "echo")
            },
            parts(&args, "echo "),
        ];
        for parts in changed {
            assert_ne!(parts.key(), base);
        }
    }

    #[test]
    fn an_entry_reads_back_what_was_written_and_garbage_is_a_miss() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path().join("shellcheck"));
        assert_eq!(cache.read("k"), None);
        let findings = vec![ShellcheckFinding {
            line: 2,
            column: 6,
            end_line: Some(2),
            end_column: None,
            level: "info".to_owned(),
            code: 2086,
            message: "Double quote.".to_owned(),
        }];
        cache.write("k", &findings);
        assert_eq!(cache.read("k"), Some(findings));
        cache.write("empty", &[]);
        assert_eq!(cache.read("empty"), Some(Vec::new()));
        fs::write(cache.dir().join("k.json"), "[1, 2").unwrap();
        assert_eq!(cache.read("k"), None);
        let names: Vec<String> = fs::read_dir(cache.dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names.len(), 2, "{names:?}");
    }
}
