use protectorate_session::{classify, explain};

/// A harness name, a newline, then the pane text a terminal sleeve showed.
pub fn terminal_pane(data: &[u8]) {
    let input = String::from_utf8_lossy(data);
    let (harness, pane) = input.split_once('\n').unwrap_or((&input, ""));
    let verdict = classify(harness, pane);
    assert_eq!(
        verdict,
        classify(harness, pane),
        "classification is not deterministic"
    );
    match explain(harness, pane) {
        None => assert!(verdict.is_none(), "a harness without rules was classified"),
        Some(explanation) => {
            let first = explanation.rules.iter().find(|(_, _, matched)| *matched);
            assert_eq!(
                verdict,
                first.map(|(id, activity, _)| (*activity, *id)),
                "explain disagrees with classify"
            );
        }
    }
}
