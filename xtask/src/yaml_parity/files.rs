//! Finds the files to compare.

use std::path::{Path, PathBuf};

use std::fs;

use anyhow::{Context, Result, bail};

/// Expands `paths` into files: a file stays as it is, and a directory gives
/// every `.yaml` and `.yml` file below it, hidden directories included, in
/// sorted order.
pub(crate) fn collect(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for path in paths {
        let metadata =
            fs::metadata(path).with_context(|| format!("cannot read {}", path.display()))?;
        if metadata.is_dir() {
            walk(path, &mut files)?;
        } else if metadata.is_file() {
            files.push(path.clone());
        } else {
            bail!("{} is neither a file nor a directory", path.display());
        }
    }
    Ok(files)
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries = fs::read_dir(dir)
        .with_context(|| format!("cannot read {}", dir.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("cannot read {}", dir.display()))?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk(&path, files)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "yaml" || extension == "yml")
        {
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn a_directory_gives_its_yaml_files_in_sorted_order_with_hidden_ones() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for name in [
            "b.yml",
            "a.yaml",
            "notes.txt",
            "sub/c.yaml",
            ".github/workflows/d.yml",
        ] {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "a: 1\n").unwrap();
        }
        let files = collect(&[root.to_path_buf()]).unwrap();
        let names: Vec<_> = files
            .iter()
            .map(|path| {
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert_eq!(
            names,
            [".github/workflows/d.yml", "a.yaml", "b.yml", "sub/c.yaml"]
        );
    }

    #[test]
    fn a_file_is_kept_whatever_its_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workflow.txt");
        fs::write(&path, "a: 1\n").unwrap();
        assert_eq!(collect(std::slice::from_ref(&path)).unwrap(), [path]);
    }

    #[test]
    fn a_missing_path_is_an_error_that_names_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.yaml");
        let error = collect(&[path]).unwrap_err();
        assert!(format!("{error:#}").contains("missing.yaml"), "{error:#}");
    }
}
