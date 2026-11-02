pub struct Args {
    pub words: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

/// Flags that take no value; every other `--flag` takes the next word.
const SWITCHES: &[&str] = &[
    "--json",
    "--steer",
    "--help",
    "--pending",
    "--readonly",
    "--even-if-present",
    "--maintenance",
    "--envoy",
    "--check",
    "--rollback",
    "--retry-rollback",
    "--refresh",
    "--all",
    "--no-wait",
    "--wait",
    "--force",
];

impl Args {
    pub fn parse(raw: Vec<String>) -> Result<Args, String> {
        let mut words = Vec::new();
        let mut flags = Vec::new();
        let mut it = raw.into_iter();
        while let Some(a) = it.next() {
            if a == "--" {
                words.extend(it.by_ref());
                break;
            }
            if let Some(f) = a.strip_prefix("--") {
                if let Some((k, v)) = f.split_once('=') {
                    flags.push((format!("--{k}"), Some(v.to_string())));
                } else if SWITCHES.contains(&a.as_str()) {
                    flags.push((a, None));
                } else {
                    let v = it.next().ok_or_else(|| format!("{a} needs a value"))?;
                    flags.push((a, Some(v)));
                }
            } else {
                words.push(a);
            }
        }
        Ok(Args { words, flags })
    }

    pub fn flag(&self, name: &str) -> Option<&str> {
        self.flags
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .and_then(|(_, v)| v.as_deref())
    }

    pub fn all(&self, name: &str) -> Vec<String> {
        self.flags
            .iter()
            .filter(|(k, _)| k == name)
            .filter_map(|(_, v)| v.clone())
            .collect()
    }

    pub fn has(&self, name: &str) -> bool {
        self.flags.iter().any(|(k, _)| k == name)
    }

    pub fn word(&self, i: usize, what: &str) -> Result<&str, String> {
        self.words
            .get(i)
            .map(String::as_str)
            .ok_or_else(|| format!("missing {what}"))
    }

    pub fn rest(&self, from: usize, what: &str) -> Result<String, String> {
        let t = self.words.get(from..).unwrap_or(&[]).join(" ");
        if t.is_empty() {
            return Err(format!("missing {what}"));
        }
        Ok(t)
    }

    pub fn num(&self, name: &str) -> Result<Option<u64>, String> {
        self.flag(name)
            .map(|v| {
                v.parse()
                    .map_err(|_| format!("{name} {v:?} is not a number"))
            })
            .transpose()
    }
}
