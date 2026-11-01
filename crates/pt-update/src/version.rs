use std::cmp::Ordering;
use std::fmt;

pt_json::string_enum! {
    /// Stable takes tags without a pre-release part (`v1.2.3`). Dev takes every tag
    /// (`v1.3.0-dev.4` and stable ones), so it is never behind stable.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Channel {
        Stable = "stable",
        Dev = "dev",
    }
}

impl Channel {
    pub fn admits(self, v: &Version) -> bool {
        match self {
            Channel::Stable => v.pre.is_empty(),
            Channel::Dev => true,
        }
    }
}

/// A release tag, `vMAJOR.MINOR.PATCH` with an optional `-PRE` of dot-separated identifiers,
/// ordered by semver precedence. Build metadata (`+...`) is not accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Vec<String>,
}

fn number(s: &str) -> Option<u64> {
    if s.is_empty() || (s.len() > 1 && s.starts_with('0')) || !s.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    s.parse().ok()
}

impl Version {
    pub fn parse(tag: &str) -> Option<Version> {
        let rest = tag.strip_prefix('v')?;
        let (core, pre) = match rest.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (rest, None),
        };
        let mut parts = core.split('.');
        let major = number(parts.next()?)?;
        let minor = number(parts.next()?)?;
        let patch = number(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        let mut ids = Vec::new();
        if let Some(pre) = pre {
            for id in pre.split('.') {
                // A numeric identifier has no leading zero but may be any length; cmp_id
                // compares it without parsing.
                let numeric = id.bytes().all(|b| b.is_ascii_digit());
                let ok = !id.is_empty()
                    && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    && (!numeric || id == "0" || !id.starts_with('0'));
                if !ok {
                    return None;
                }
                ids.push(id.to_string());
            }
        }
        Some(Version {
            major,
            minor,
            patch,
            pre: ids,
        })
    }

    pub fn channel(&self) -> Channel {
        if self.pre.is_empty() {
            Channel::Stable
        } else {
            Channel::Dev
        }
    }
}

fn cmp_id(a: &str, b: &str) -> Ordering {
    let numeric = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    match (numeric(a), numeric(b)) {
        // No leading zeros, so a longer number is larger.
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.cmp(b)),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.cmp(b),
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Version) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => {
                    for (a, b) in self.pre.iter().zip(&other.pre) {
                        let o = cmp_id(a, b);
                        if o != Ordering::Equal {
                            return o;
                        }
                    }
                    self.pre.len().cmp(&other.pre.len())
                }
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Version) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre.join("."))?;
        }
        Ok(())
    }
}
