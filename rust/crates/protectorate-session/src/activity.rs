//! Working, blocked or idle for a terminal harness, read from the bottom of
//! its screen. Rules are tried in order per harness, so the first match wins
//! and a blocking dialog outranks the working indicator behind it. Only the
//! last few non-empty lines are examined: older lines are history.
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Working,
    Blocked,
    Idle,
}

impl Activity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Idle => "idle",
        }
    }
}

impl fmt::Display for Activity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One condition on the examined lines, which are lowercased.
enum Pat {
    /// The text appears anywhere in the region.
    Has(&'static str),
    /// Some line starts with the text once leading blanks are dropped.
    Line(&'static str),
    /// Some line holds a braille spinner cell and the word.
    Spinner(&'static str),
}

struct Rule {
    id: &'static str,
    activity: Activity,
    /// How many of the bottom non-empty lines the rule may read.
    last: usize,
    /// Every pattern must hold.
    all: &'static [Pat],
}

use Activity::{Blocked, Idle, Working};
use Pat::{Has, Line, Spinner};

const fn rule(id: &'static str, activity: Activity, last: usize, all: &'static [Pat]) -> Rule {
    Rule {
        id,
        activity,
        last,
        all,
    }
}

const CLAUDE: &[Rule] = &[
    rule(
        "claude.permission",
        Blocked,
        14,
        &[Has("do you want"), Has("esc to cancel")],
    ),
    rule(
        "claude.confirm",
        Blocked,
        14,
        &[Has("enter to confirm"), Has("esc to cancel")],
    ),
    rule(
        "claude.select",
        Blocked,
        14,
        &[Has("enter to select"), Has("esc to cancel")],
    ),
    rule("claude.interrupt", Working, 8, &[Has("esc to interrupt")]),
    rule("claude.prompt", Idle, 8, &[Line("❯")]),
];

const CODEX: &[Rule] = &[
    rule(
        "codex.confirm",
        Blocked,
        14,
        &[Has("press enter to confirm or esc to cancel")],
    ),
    rule(
        "codex.update",
        Blocked,
        14,
        &[Has("enter continue"), Has("esc skip")],
    ),
    rule("codex.trust", Blocked, 14, &[Has("do you trust")]),
    rule("codex.allow", Blocked, 14, &[Has("allow command?")]),
    rule("codex.interrupt", Working, 8, &[Has("esc to interrupt")]),
    rule("codex.prompt", Idle, 8, &[Line("›")]),
];

const GROK: &[Rule] = &[
    rule("grok.yolo", Blocked, 4, &[Has("ctrl+o:yolo")]),
    rule("grok.choice", Blocked, 12, &[Has("(○)")]),
    rule("grok.cancel", Working, 3, &[Has("ctrl+c:cancel")]),
    rule("grok.prompt", Idle, 8, &[Line("│ ❯")]),
];

const PI: &[Rule] = &[
    rule("pi.working", Working, 12, &[Spinner("working")]),
    rule("pi.footer", Idle, 3, &[Has("%/")]),
];

fn rules(harness: &str) -> Option<&'static [Rule]> {
    match harness {
        "claude" => Some(CLAUDE),
        "codex" => Some(CODEX),
        "grok" => Some(GROK),
        "pi" => Some(PI),
        _ => None,
    }
}

fn is_spinner(c: char) -> bool {
    ('\u{2800}'..='\u{28ff}').contains(&c)
}

/// The last `n` non-empty lines, lowercased, in screen order.
fn bottom(pane: &str, n: usize) -> Vec<String> {
    let mut lines: Vec<String> = pane
        .lines()
        .rev()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .take(n)
        .map(str::to_lowercase)
        .collect();
    lines.reverse();
    lines
}

impl Pat {
    fn holds(&self, lines: &[String]) -> bool {
        match self {
            Self::Has(text) => lines.iter().any(|line| line.contains(text)),
            Self::Line(text) => lines.iter().any(|line| line.trim_start().starts_with(text)),
            Self::Spinner(word) => lines
                .iter()
                .any(|line| line.contains(word) && line.chars().any(is_spinner)),
        }
    }
}

impl Rule {
    fn matches(&self, pane: &str) -> bool {
        let lines = bottom(pane, self.last);
        self.all.iter().all(|pat| pat.holds(&lines))
    }
}

/// The activity of `harness` (claude, codex, grok or pi) and the id of the
/// rule that decided it, or None when no rule matches or the harness has none.
pub fn classify(harness: &str, pane: &str) -> Option<(Activity, &'static str)> {
    rules(harness)?
        .iter()
        .find(|rule| rule.matches(pane))
        .map(|rule| (rule.activity, rule.id))
}

/// Every rule of `harness` with whether it matched, the first match being the
/// decision, and the widest region any rule reads.
pub struct Explanation {
    pub rules: Vec<(&'static str, Activity, bool)>,
    pub region: Vec<String>,
}

pub fn explain(harness: &str, pane: &str) -> Option<Explanation> {
    let rules = rules(harness)?;
    let widest = rules.iter().map(|rule| rule.last).max().unwrap_or(0);
    Some(Explanation {
        rules: rules
            .iter()
            .map(|rule| (rule.id, rule.activity, rule.matches(pane)))
            .collect(),
        region: bottom(pane, widest),
    })
}
