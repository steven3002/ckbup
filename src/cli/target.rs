/// A `<tool>@<version>` target for the commands that act on one installed version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedTarget {
    pub tool: String,
    pub version: String,
}

/// Splits `<tool>[@<version>]`; a version given without the `v` prefix gets one,
/// matching the tag format of every supported repository.
pub fn parse_target(target: &str) -> (&str, Option<String>) {
    match target.split_once('@') {
        Some((tool, version)) if version.starts_with(|c: char| c.is_ascii_digit()) => {
            (tool, Some(format!("v{version}")))
        }
        Some((tool, version)) => (tool, Some(version.to_string())),
        None => (target, None),
    }
}

/// Parses the target of `ckbup use`, which must name a version.
pub fn use_target(target: &str) -> Result<VersionedTarget, String> {
    versioned(target, "ckbup use fnn@v0.10.1")
}

/// Parses the target of `ckbup remove`, which must name a version.
pub fn remove_target(target: &str) -> Result<VersionedTarget, String> {
    versioned(target, "ckbup remove fnn@v0.9.1")
}

/// Parses the tool of `ckbup update`, which always moves to the latest
/// stable release and so takes no version.
pub fn update_tool(target: &str) -> Result<String, String> {
    if target.contains('@') {
        return Err(format!(
            "expected a tool without a version, for example: ckbup update fnn. To install a specific version, run: ckbup install {target}"
        ));
    }
    Ok(target.to_string())
}

fn versioned(target: &str, example: &str) -> Result<VersionedTarget, String> {
    match parse_target(target) {
        (tool, Some(version)) if !tool.is_empty() && !version.is_empty() => Ok(VersionedTarget {
            tool: tool.to_string(),
            version,
        }),
        _ => Err(format!("expected <tool>@<version>, for example: {example}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tool_and_optional_version() {
        assert_eq!(parse_target("ckb-cli"), ("ckb-cli", None));
        assert_eq!(
            parse_target("ckb-cli@v2.0.0"),
            ("ckb-cli", Some("v2.0.0".to_string()))
        );
        assert_eq!(
            parse_target("ckb-cli@2.0.0"),
            ("ckb-cli", Some("v2.0.0".to_string()))
        );
    }

    #[test]
    fn use_and_remove_add_the_v_prefix() {
        let expected = VersionedTarget {
            tool: "fnn".to_string(),
            version: "v0.9.1".to_string(),
        };
        assert_eq!(use_target("fnn@0.9.1").unwrap(), expected);
        assert_eq!(use_target("fnn@v0.9.1").unwrap(), expected);
        assert_eq!(remove_target("fnn@0.9.1").unwrap(), expected);
        assert_eq!(remove_target("fnn@v0.9.1").unwrap(), expected);
    }

    #[test]
    fn update_takes_a_tool_without_a_version() {
        assert_eq!(update_tool("fnn").unwrap(), "fnn");
        assert_eq!(
            update_tool("fnn@v0.10.1").unwrap_err(),
            "expected a tool without a version, for example: ckbup update fnn. To install a specific version, run: ckbup install fnn@v0.10.1"
        );
    }

    #[test]
    fn use_and_remove_need_a_version() {
        for target in ["fnn", "fnn@", "@v0.9.1"] {
            assert_eq!(
                use_target(target).unwrap_err(),
                "expected <tool>@<version>, for example: ckbup use fnn@v0.10.1"
            );
            assert_eq!(
                remove_target(target).unwrap_err(),
                "expected <tool>@<version>, for example: ckbup remove fnn@v0.9.1"
            );
        }
    }
}
