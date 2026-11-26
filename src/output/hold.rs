//! The lines that tell the user how a tool's hold changed.

use crate::store::Hold;

/// What an activation of `tool` at `version` did to its hold: it set the hold,
/// or released one that was set. Keeping a hold says nothing.
pub fn hold_report(tool: &str, version: &str, hold: Hold, was_held: bool) -> String {
    match hold {
        Hold::Set => format!(
            "{tool} is held at {version}; ckbup update skips it until you run: ckbup update {tool}\n"
        ),
        Hold::Release if was_held => format!("{tool} now follows the latest stable release\n"),
        Hold::Keep | Hold::Release => String::new(),
    }
}

/// The line `update` prints for a held tool that it leaves alone.
pub fn held_skip(tool: &str, version: Option<&str>) -> String {
    let at = version
        .map(|version| format!(" {version}"))
        .unwrap_or_default();
    format!("{tool}{at} is held; move it to the latest stable release with: ckbup update {tool}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_when_a_hold_is_set_or_released() {
        assert_eq!(
            hold_report("fnn", "v0.9.1", Hold::Set, false),
            "fnn is held at v0.9.1; ckbup update skips it until you run: ckbup update fnn\n"
        );
        assert_eq!(
            hold_report("fnn", "v0.9.1", Hold::Set, true),
            hold_report("fnn", "v0.9.1", Hold::Set, false)
        );
        assert_eq!(
            hold_report("fnn", "v0.10.1", Hold::Release, true),
            "fnn now follows the latest stable release\n"
        );
        assert_eq!(hold_report("fnn", "v0.10.1", Hold::Release, false), "");
        assert_eq!(hold_report("fnn", "v0.10.1", Hold::Keep, true), "");
    }

    #[test]
    fn names_the_command_that_moves_a_held_tool() {
        assert_eq!(
            held_skip("fnn", Some("v0.9.1")),
            "fnn v0.9.1 is held; move it to the latest stable release with: ckbup update fnn\n"
        );
        assert_eq!(
            held_skip("fnn", None),
            "fnn is held; move it to the latest stable release with: ckbup update fnn\n"
        );
    }
}
