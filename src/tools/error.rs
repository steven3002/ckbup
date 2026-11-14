use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("unknown tool `{name}`; supported tools: {supported}")]
    Unknown { name: String, supported: String },
    #[error("no official {tool} binary for {os}/{arch}")]
    NoOfficialBinary {
        tool: &'static str,
        os: &'static str,
        arch: &'static str,
    },
    #[error(
        "{tool} {tag} has no release archive for {os}/{arch}; archives in this release:\n{archives}"
    )]
    NoMatchingAsset {
        tool: &'static str,
        tag: String,
        os: &'static str,
        arch: &'static str,
        archives: String,
    },
}
