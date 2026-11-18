use std::path::PathBuf;

/// Reports a finished activation: where the executables of `tool` are now,
/// and which ones the version does not include and so were taken out of `bin/`.
pub fn activation_report(
    tool: &str,
    version: &str,
    on_path: &[PathBuf],
    removed: &[PathBuf],
) -> String {
    let paths: Vec<String> = on_path
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    let mut report = format!("{tool} {version} is active: {}\n", paths.join(", "));
    for path in removed {
        let file = path
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default();
        report.push_str(&format!(
            "{tool} {version} does not include {file}; removed {}\n",
            path.display()
        ));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn names_the_active_files_and_the_ones_taken_out() {
        let bin = Path::new("/home/u/.ckbup/bin");
        assert_eq!(
            activation_report(
                "fnn",
                "v0.9.1",
                &[bin.join("fnn"), bin.join("fnn-cli")],
                &[]
            ),
            format!(
                "fnn v0.9.1 is active: {}, {}\n",
                bin.join("fnn").display(),
                bin.join("fnn-cli").display()
            )
        );
        assert_eq!(
            activation_report("fnn", "v0.9.1", &[bin.join("fnn")], &[bin.join("fnn-cli")]),
            format!(
                "fnn v0.9.1 is active: {}\nfnn v0.9.1 does not include fnn-cli; removed {}\n",
                bin.join("fnn").display(),
                bin.join("fnn-cli").display()
            )
        );
    }
}
