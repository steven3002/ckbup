use std::cmp::Ordering;

/// Compares two release tags so that sorting with it puts the newest first.
///
/// Tags of the form `v?MAJOR.MINOR.PATCH[-suffix]` compare by their three numbers.
/// At equal numbers the tag without a suffix is newer, and suffixes compare as
/// text. Tags of any other form come after all of these, by name. Neither
/// directory listings nor the GitHub API promise an order, so every list of
/// versions is sorted with this.
pub fn newest_first(a: &str, b: &str) -> Ordering {
    match (Version::parse(a), Version::parse(b)) {
        (Some(left), Some(right)) => right.cmp(&left).then_with(|| a.cmp(b)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.cmp(b),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Version<'a> {
    numbers: [u64; 3],
    suffix: Option<&'a str>,
}

impl<'a> Version<'a> {
    fn parse(tag: &'a str) -> Option<Self> {
        let rest = tag.strip_prefix('v').unwrap_or(tag);
        let (core, suffix) = match rest.split_once('-') {
            Some((core, suffix)) => (core, Some(suffix)),
            None => (rest, None),
        };
        let mut parts = core.split('.');
        let mut numbers = [0; 3];
        for number in &mut numbers {
            let part = parts.next()?;
            if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            *number = part.parse().ok()?;
        }
        if parts.next().is_some() {
            return None;
        }
        Some(Version { numbers, suffix })
    }
}

impl Ord for Version<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.numbers
            .cmp(&other.numbers)
            .then_with(|| match (self.suffix, other.suffix) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(left), Some(right)) => left.cmp(right),
            })
    }
}

impl PartialOrd for Version<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(tags: &[&str]) -> Vec<String> {
        let mut tags: Vec<String> = tags.iter().map(|tag| tag.to_string()).collect();
        tags.sort_by(|a, b| newest_first(a, b));
        tags
    }

    #[test]
    fn compares_numbers_numerically() {
        assert_eq!(sorted(&["v0.9.1", "v0.10.1"]), ["v0.10.1", "v0.9.1"]);
        assert_eq!(
            sorted(&["v0.9.0", "v0.21.0", "v0.210.0"]),
            ["v0.210.0", "v0.21.0", "v0.9.0"]
        );
    }

    #[test]
    fn plain_tag_is_newer_than_its_suffixed_tags() {
        assert_eq!(
            sorted(&["v1.0.0-rc1", "v1.0.0", "v1.0.0-rc2"]),
            ["v1.0.0", "v1.0.0-rc2", "v1.0.0-rc1"]
        );
        assert_eq!(
            sorted(&["v0.107.0-alpha", "v0.107.0", "v0.107.0-alpha-3"]),
            ["v0.107.0", "v0.107.0-alpha-3", "v0.107.0-alpha"]
        );
    }

    #[test]
    fn parses_tags_without_a_v() {
        assert_eq!(
            Version::parse("0.116.1"),
            Some(Version {
                numbers: [0, 116, 1],
                suffix: None
            })
        );
        assert_eq!(
            sorted(&["v0.117.0", "0.116.1", "v0.116.0"]),
            ["v0.117.0", "0.116.1", "v0.116.0"]
        );
    }

    #[test]
    fn unparseable_names_sort_last_by_name() {
        assert_eq!(
            sorted(&["nightly", "v0.1.0", "latest", "v1.0", "v2.0.0"]),
            ["v2.0.0", "v0.1.0", "latest", "nightly", "v1.0"]
        );
        for name in ["v1.0", "v1.0.0.0", "v1..0", "vx.1.0", "", "v"] {
            assert_eq!(Version::parse(name), None, "{name}");
        }
    }
}
