/// Strings from this machine are bounded and stripped of control characters before they leave it.
pub(crate) fn clean(s: &str, max: usize) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        let c = if c.is_control() { ' ' } else { c };
        if out.len() + c.len_utf8() > max {
            break;
        }
        out.push(c);
    }
    out
}
