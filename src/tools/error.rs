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
}
