//! pt-history-probe: indexes this machine's native harness logs (newest first, up to a cap)
//! into a fresh index, reports counts, size, ingest throughput and query latency, then rebuilds
//! a second index from the same frozen sources with a different commit pattern and checks the
//! results are identical. Snippets that may hold a secret are withheld.
//!
//! ```text
//! pt-history-probe --index DIR [--cap 1G] [--budget 1G] [--sleeve ID] [--home DIR]
//!     [--runs 20] [--query Q]...
//! pt-history-probe --index DIR --read SESSION [--after DOC] [--budget BYTES]
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant, UNIX_EPOCH};

use pt_history::{
    DEFAULT_BUDGET, DocId, Filters, Hit, Index, Query, Selector, Source, SourceKind, brief_query,
    discover,
};

const QUERIES: &[&str] = &[
    "journal codec torn tail",
    "flutter analyze",
    "headscale register",
    "bwrap",
    "READY",
];
const BRIEF_TASK: &str = "Fix torn-tail recovery in the pt-journal codec and run cargo test";

struct Args {
    index: PathBuf,
    cap: u64,
    budget: u64,
    home: PathBuf,
    runs: usize,
    queries: Vec<String>,
    sleeve: Option<String>,
    read: Option<String>,
    after: Option<u64>,
}

fn usage() -> String {
    "usage: pt-history-probe --index DIR [--cap BYTES[K|M|G]] [--budget BYTES[K|M|G]] [--sleeve ID] [--home DIR] [--runs N] [--query Q]...\n       pt-history-probe --index DIR --read SESSION [--after DOC] [--budget BYTES[K|M|G]]"
        .into()
}

fn size(s: &str) -> Option<u64> {
    let (num, mul) = match s.as_bytes().last()? {
        b'K' | b'k' => (&s[..s.len() - 1], 1 << 10),
        b'M' | b'm' => (&s[..s.len() - 1], 1 << 20),
        b'G' | b'g' => (&s[..s.len() - 1], 1 << 30),
        _ => (s, 1),
    };
    num.parse::<u64>().ok()?.checked_mul(mul)
}

fn args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let mut index = None;
    let mut cap = 1 << 30;
    let mut budget = DEFAULT_BUDGET;
    let mut home = std::env::var_os("HOME").map(PathBuf::from);
    let mut sleeve = None;
    let mut read = None;
    let mut after = None;
    let mut runs = 20;
    let mut queries = Vec::new();
    while let Some(a) = it.next() {
        let mut val = || {
            it.next()
                .ok_or_else(|| format!("{a} needs a value\n{}", usage()))
        };
        match a.as_str() {
            "--index" => index = Some(PathBuf::from(val()?)),
            "--cap" => cap = size(&val()?).ok_or_else(usage)?,
            "--budget" => budget = size(&val()?).ok_or_else(usage)?,
            "--sleeve" => sleeve = Some(val()?),
            "--read" => read = Some(val()?),
            "--after" => after = Some(val()?.parse().map_err(|_| usage())?),
            "--home" => home = Some(PathBuf::from(val()?)),
            "--runs" => runs = val()?.parse().map_err(|_| usage())?,
            "--query" => queries.push(val()?),
            _ => return Err(usage()),
        }
    }
    Ok(Args {
        index: index.ok_or_else(usage)?,
        cap,
        budget,
        home: match (&read, home) {
            (Some(_), home) => home.unwrap_or_default(),
            (None, home) => home.ok_or("no --home and no HOME")?,
        },
        sleeve,
        read,
        after,
        runs: runs.max(1),
        queries: if queries.is_empty() {
            QUERIES.iter().map(|q| q.to_string()).collect()
        } else {
            queries
        },
    })
}

fn mb(b: u64) -> f64 {
    b as f64 / (1 << 20) as f64
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Newest logs first until `cap` bytes, each frozen at its current size.
fn select(home: &Path, cap: u64) -> Result<Vec<Source>, String> {
    let roots = [
        (SourceKind::Claude, ".claude/projects"),
        (SourceKind::Codex, ".codex/sessions"),
        (SourceKind::Grok, ".grok/sessions"),
        (SourceKind::Pi, ".pi/agent/sessions"),
    ];
    let mut all = Vec::new();
    for (kind, rel) in roots {
        let root = home.join(rel);
        if !root.is_dir() {
            continue;
        }
        for s in discover(kind, &root).map_err(|e| e.to_string())? {
            let Ok(meta) = std::fs::metadata(&s.path) else {
                continue;
            };
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok());
            all.push((mtime.unwrap_or_default(), meta.len(), s));
        }
    }
    all.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.path.cmp(&b.2.path)));
    let mut total = 0u64;
    let mut out = Vec::new();
    for (_, len, mut s) in all {
        if total >= cap {
            break;
        }
        if len == 0 {
            continue;
        }
        let take = len.min(cap - total);
        s.limit = Some(take);
        total += take;
        out.push(s);
    }
    out.sort_by(|a, b| (a.kind, &a.path).cmp(&(b.kind, &b.path)));
    Ok(out)
}

#[derive(Default)]
struct Totals {
    bytes: u64,
    lines: u64,
    unknown: u64,
    docs: u64,
    overlong: u64,
}

fn build(
    dir: &Path,
    sources: &[Source],
    commit_each: bool,
    budget: u64,
    report_cap: bool,
) -> Result<(Index, Totals), String> {
    if dir.join("manifest").exists() {
        return Err(format!(
            "{} already holds an index; use a fresh --index",
            dir.display()
        ));
    }
    let mut ix = Index::open(dir, budget).map_err(|e| e.to_string())?;
    let mut t = Totals::default();
    for s in sources {
        let got = match ix.ingest(s) {
            Ok(got) => got,
            Err(e) if retry_test_failure(&e) => ix.ingest(s).map_err(|e| e.to_string())?,
            Err(e) => return Err(e.to_string()),
        };
        t.bytes += got.bytes;
        t.lines += got.lines;
        t.unknown += got.unknown;
        t.docs += got.docs;
        t.overlong += u64::from(got.overlong);
        if commit_each {
            commit(&mut ix)?;
        }
        if report_cap {
            let names = ix.capped_sleeves();
            if names.is_empty() {
                println!("capped: none");
            } else {
                println!("capped: {}", names.join(","));
            }
        }
    }
    commit(&mut ix)?;
    Ok((ix, t))
}

fn retry_test_failure(e: &std::io::Error) -> bool {
    if cfg!(debug_assertions)
        && std::env::var_os("PT_TEST_HISTORY_FAIL_COMMIT").is_some()
        && e.to_string().starts_with("injected history ")
    {
        eprintln!("retrying {e}");
        true
    } else {
        false
    }
}

fn commit(ix: &mut Index) -> Result<(), String> {
    match ix.commit() {
        Ok(()) => Ok(()),
        Err(e) if retry_test_failure(&e) => ix.commit().map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

fn day(ms: u64) -> String {
    let days = (ms / 86_400_000) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn shown(h: &Hit) -> String {
    let lower = h.snippet.text.to_lowercase();
    if ["key", "token", "secret"].iter().any(|w| lower.contains(w)) {
        return "[snippet withheld: may hold a secret]".into();
    }
    h.snippet.marked("[", "]")
}

/// Every `docs / 20`-th doc must be found by a phrase of its first words within its session.
fn recall(ix: &Index, docs: u64) -> Result<(usize, usize), String> {
    let (mut found, mut sampled) = (0, 0);
    let step = (docs / 20).max(1);
    let mut id = 0;
    while sampled < 20 && id < docs * 4 + 100 {
        let (got, _) = ix
            .read(&Selector::Doc(DocId(id)), None, 1)
            .map_err(|e| e.to_string())?;
        id += step;
        let Some(doc) = got.into_iter().next() else {
            continue;
        };
        let mut words = Vec::new();
        pt_history::token::tokenize(&doc.text, |t| {
            if !t.whole && words.len() < 3 {
                words.push(t.term.to_owned());
            }
        });
        if words.is_empty() {
            continue;
        }
        sampled += 1;
        let filters = Filters {
            session: Some(doc.session.clone()),
            ..Filters::default()
        };
        let hits = ix
            .search(&Query::phrase(words.join(" ")), &filters, 10_000)
            .map_err(|e| e.to_string())?;
        if hits.iter().any(|h| h.doc.id == doc.id) {
            found += 1;
        }
    }
    Ok((found, sampled))
}

/// A comparable fingerprint of a result list.
fn fingerprint(hits: &[Hit]) -> Vec<(u64, u64, String)> {
    hits.iter()
        .map(|h| (h.doc.id.0, h.score.to_bits(), h.snippet.text.clone()))
        .collect()
}

/// One page of a session: `doc ID TURN` lines, then `more true` or `more false`.
fn read_page(index: &Path, session: &str, after: Option<u64>, budget: u64) -> Result<(), String> {
    let ix = Index::open(index, budget).map_err(|e| e.to_string())?;
    let (docs, more) = ix
        .read(
            &Selector::Turns {
                session: session.to_owned(),
                from: 0,
                to: u32::MAX,
                after,
            },
            None,
            pt_history::READ_DOCS,
        )
        .map_err(|e| e.to_string())?;
    for d in &docs {
        println!("doc {} {}", d.id.0, d.turn);
    }
    println!("more {more}");
    Ok(())
}

fn run() -> Result<bool, String> {
    let a = args()?;
    if a.after.is_some() && a.read.is_none() {
        return Err(usage());
    }
    if let Some(session) = &a.read {
        read_page(&a.index, session, a.after, a.budget)?;
        return Ok(true);
    }
    let mut sources = select(&a.home, a.cap)?;
    if let Some(sleeve) = &a.sleeve {
        for s in &mut sources {
            s.sleeve = Some(sleeve.clone());
        }
    }
    let report_cap = a.sleeve.is_some();
    #[cfg(debug_assertions)]
    if let Some(marker) = std::env::var_os("PT_TEST_HISTORY_DISCOVERY_BARRIER") {
        let marker = PathBuf::from(marker);
        std::fs::write(&marker, b"ready").map_err(|e| format!("write discovery barrier: {e}"))?;
        let until = Instant::now() + Duration::from_secs(15);
        while !marker.with_extension("continue").exists() {
            if Instant::now() >= until {
                return Err("discovery barrier timed out".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let selected: u64 = sources.iter().filter_map(|s| s.limit).sum();
    let mut by_kind = std::collections::BTreeMap::new();
    for s in &sources {
        *by_kind.entry(s.kind.as_str()).or_insert(0u64) += 1;
    }
    let kinds: Vec<String> = by_kind.iter().map(|(k, n)| format!("{k} {n}")).collect();
    println!(
        "selected: {} files, {:.1} MB ({})",
        sources.len(),
        mb(selected),
        kinds.join(", ")
    );
    if sources.is_empty() {
        println!("summary: files=0");
        return Ok(true);
    }

    let t0 = Instant::now();
    let (ix, t) = build(&a.index.join("a"), &sources, false, a.budget, report_cap)?;
    let took = t0.elapsed();
    println!(
        "ingest: {:.1} MB in {:.2} s = {:.1} MB/s; lines {}, unknown {}, docs {}, overlong {}",
        mb(t.bytes),
        took.as_secs_f64(),
        mb(t.bytes) / took.as_secs_f64(),
        t.lines,
        t.unknown,
        t.docs,
        t.overlong
    );
    let st = ix.stats();
    println!(
        "index: docs {}, sessions {}, segments {}, terms {}, size {:.1} MB ({:.1}% of raw)",
        st.docs,
        st.sessions,
        st.segments,
        st.terms,
        mb(st.bytes),
        100.0 * st.bytes as f64 / t.bytes.max(1) as f64
    );
    drop(ix);
    let t0 = Instant::now();
    let ix = Index::open(&a.index.join("a"), a.budget).map_err(|e| e.to_string())?;
    println!("open: {:.1} ms", ms(t0.elapsed()));

    let filters = Filters::default();
    let mut with_hits = 0;
    let mut results = Vec::new();
    for q in &a.queries {
        let query = Query::all(q);
        let mut times = Vec::with_capacity(a.runs);
        let mut hits = Vec::new();
        for _ in 0..a.runs {
            let t0 = Instant::now();
            hits = ix.search(&query, &filters, 5).map_err(|e| e.to_string())?;
            times.push(t0.elapsed());
        }
        times.sort();
        let p50 = times[times.len() / 2];
        let p95 = times[(times.len() * 95 / 100).min(times.len() - 1)];
        println!(
            "query {q:?}: top {}, p50 {:.2} ms, p95 {:.2} ms",
            hits.len(),
            ms(p50),
            ms(p95)
        );
        for (i, h) in hits.iter().take(3).enumerate() {
            let harness = h.doc.harness.map_or("hub", |h| h.as_str());
            println!(
                "  {}. {:.2} {harness} {} turn {} {} {}: {}",
                i + 1,
                h.score,
                h.doc.session.get(..8).unwrap_or(&h.doc.session),
                h.doc.turn,
                h.doc.role,
                day(h.doc.time_ms),
                shown(h)
            );
        }
        if !hits.is_empty() {
            with_hits += 1;
        }
        results.push(fingerprint(&hits));
    }

    let (found, sampled) = recall(&ix, st.docs)?;
    println!("recall: {found}/{sampled} sampled docs found by a phrase of their own words");

    let t0 = Instant::now();
    let brief = ix
        .search(&brief_query(BRIEF_TASK), &Filters::default(), 3)
        .map_err(|e| e.to_string())?;
    println!(
        "brief_query {BRIEF_TASK:?}: {} hits in {:.2} ms",
        brief.len(),
        ms(t0.elapsed())
    );
    for s in sources.iter().rev().take(3) {
        if let Some(u) = ix.context_usage(&s.session) {
            println!(
                "context: {} {} tokens {} window {} turns {} compactions {}",
                s.kind,
                s.session.get(..8).unwrap_or(&s.session),
                u.tokens,
                u.window.map_or("-".into(), |w| w.to_string()),
                u.turns,
                u.compactions
            );
        }
    }

    let t0 = Instant::now();
    let (rb, _) = build(&a.index.join("b"), &sources, true, a.budget, report_cap)?;
    let rst = rb.stats();
    let mut same = rst.docs == st.docs && rst.sessions == st.sessions;
    let mut compared = 0;
    for (q, want) in a.queries.iter().zip(&results) {
        let got = rb
            .search(&Query::all(q), &filters, 5)
            .map_err(|e| e.to_string())?;
        compared += got.len();
        same &= fingerprint(&got) == *want;
    }
    let brief_again = rb
        .search(&brief_query(BRIEF_TASK), &Filters::default(), 3)
        .map_err(|e| e.to_string())?;
    same &= fingerprint(&brief_again) == fingerprint(&brief);
    println!(
        "rebuild: {} in {:.2} s, {} segments, {compared} hits compared",
        if same { "identical" } else { "DIFFERENT" },
        t0.elapsed().as_secs_f64(),
        rst.segments
    );
    println!(
        "summary: files={} docs={} recall={found}/{sampled} queries_with_hits={with_hits}/{} rebuild={}",
        sources.len(),
        st.docs,
        a.queries.len(),
        if same { "identical" } else { "different" }
    );
    Ok(same && found == sampled)
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(2),
        Err(e) => {
            eprintln!("pt-history-probe: {e}");
            ExitCode::FAILURE
        }
    }
}
