//! Prints the live quota reading of every subscription on this machine, never a secret.
//!
//! `pt-quota-probe [--home DIR] [--local URL] [--pi-provider NAME] [--claude-url URL]
//! [--codex-url URL] [--grok-url URL] [--allow claude:opus,pi_local]`
//! `--allow` takes harness names, optional `:model` on each.
//! One line per subscription: `<name> <fresh|stale|unknown|error> plan=<plan> [<window> <used>%
//! resets <time>]... note=<note>`. `PT_LOCAL_URL` also configures the local endpoint; without
//! one, the local reader uses the `baseUrl` of Pi provider `--pi-provider` (default
//! `PT_E2E_PI_PROVIDER`, the variable the e2e tests read; no provider and no URL reads nothing).

use std::path::PathBuf;
use std::process::ExitCode;

use pt_api::{Harness, Mode, Quota};
use pt_quota::{
    Cache, Candidate, Options, Reader, TaskSize, claude::Claude, codex::Codex, grok::Grok,
    local::Local,
};

fn main() -> ExitCode {
    let mut home: Option<PathBuf> = None;
    let mut local = std::env::var("PT_LOCAL_URL").unwrap_or_default();
    let mut pi_provider = std::env::var("PT_E2E_PI_PROVIDER").unwrap_or_default();
    let mut pi_model = String::new();
    let mut allow = None;
    let mut test_cache_ms = None;
    let (mut claude_url, mut codex_url, mut grok_url) = (None, None, None);
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let Some(value) = args.next() else {
            eprintln!("pt-quota-probe: {flag} needs a value");
            return ExitCode::from(2);
        };
        match flag.as_str() {
            "--home" => home = Some(PathBuf::from(value)),
            "--local" => local = value,
            "--pi-provider" => pi_provider = value,
            "--pi-model" => pi_model = value,
            "--test-cache-ms" if cfg!(debug_assertions) => {
                test_cache_ms = value.parse::<u64>().ok().filter(|ms| *ms > 0);
                if test_cache_ms.is_none() {
                    eprintln!("pt-quota-probe: invalid test cache interval");
                    return ExitCode::from(2);
                }
            }
            "--allow" => {
                let mut candidates = Vec::new();
                for spec in value.split(',') {
                    if spec.is_empty() {
                        continue;
                    }
                    let (name, model) = spec.split_once(':').unwrap_or((spec, ""));
                    let harness = match name {
                        "claude" => Harness::Claude,
                        "codex" => Harness::Codex,
                        "grok" => Harness::Grok,
                        "pi_local" => Harness::PiLocal,
                        "pi_vendor" => Harness::PiVendor,
                        "antigravity" => Harness::Antigravity,
                        _ => {
                            eprintln!("pt-quota-probe: unknown harness {name}");
                            return ExitCode::from(2);
                        }
                    };
                    candidates.push(Candidate {
                        harness,
                        model: model.to_string(),
                    });
                }
                allow = Some(candidates);
            }
            "--claude-url" => claude_url = Some(value),
            "--codex-url" => codex_url = Some(value),
            "--grok-url" => grok_url = Some(value),
            _ => {
                eprintln!("pt-quota-probe: unknown flag {flag}");
                return ExitCode::from(2);
            }
        }
    }
    let readers: Vec<Box<dyn Reader>> = vec![
        Box::new(Claude {
            home: home.clone(),
            base_url: claude_url,
        }),
        Box::new(Codex {
            home: home.clone(),
            base_url: codex_url,
        }),
        Box::new(Grok {
            home: home.clone(),
            base_url: grok_url,
        }),
        Box::new(Local {
            base_url: local,
            home,
            provider: pi_provider,
            model: pi_model,
        }),
    ];
    let mut options = Options::default();
    if let Some(ms) = test_cache_ms {
        let interval = std::time::Duration::from_millis(ms);
        options.ttl = interval;
        options.backoff_base = interval;
        options.stale_after = interval.saturating_mul(2);
    }
    let mut cache = Cache::new(readers, options);
    let now = pt_quota::now_ms();
    cache.refresh(now, false);
    if let Some(ms) = test_cache_ms {
        let earlier = now.saturating_sub(86_400_000);
        cache.refresh(earlier, true);
        std::thread::sleep(std::time::Duration::from_millis(ms.saturating_add(50)));
        cache.refresh(earlier, false);
    }
    let quotas = cache.quotas();
    for q in &quotas {
        println!("{}", line(q, now));
    }
    if let Some(ms) = test_cache_ms {
        std::thread::sleep(std::time::Duration::from_millis(
            ms.saturating_mul(2).saturating_add(50),
        ));
        let aged = cache.quotas();
        for q in &aged {
            println!("aged {}", line(q, now));
        }
        if let Some(candidates) = &allow {
            println!(
                "aged {}",
                target_line(
                    candidates,
                    pt_quota::select_with_fallback(now, TaskSize::Medium, candidates, &aged)
                )
            );
        }
    }
    if let Some(candidates) = allow {
        println!(
            "{}",
            target_line(
                &candidates,
                pt_quota::select_with_fallback(now, TaskSize::Medium, &candidates, &quotas)
            )
        );
        return ExitCode::SUCCESS;
    }
    let allow = [
        Harness::Claude,
        Harness::Codex,
        Harness::Grok,
        Harness::PiLocal,
    ]
    .map(|harness| Candidate {
        harness,
        model: String::new(),
    });
    let target = pt_quota::select_target(
        now,
        TaskSize::Medium,
        &allow,
        Mode::Headless,
        &[],
        &quotas,
        &[],
    );
    match target {
        Some(t) => println!("target {:?}: {}", t.harness, t.reason),
        None => println!("target none"),
    }
    ExitCode::SUCCESS
}

fn target_line(candidates: &[Candidate], picked: Result<(usize, String), String>) -> String {
    match picked {
        Ok((idx, reason)) => {
            let cand = &candidates[idx];
            format!("target {:?}: {reason} [{}]", cand.harness, cand.model)
        }
        Err(reason) => format!("target none: {reason}"),
    }
}

fn line(q: &Quota, now_ms: u64) -> String {
    let name = q.subscription.as_str();
    let mut out = format!("{name} {}", pt_quota::state(q).word());
    if !q.plan.is_empty() {
        out += &format!(" plan={}", q.plan);
    }
    for w in &q.windows {
        out += &format!(" [{}", w.label);
        if let Some(u) = w.used_pct {
            out += &format!(" {u:.0}%");
        }
        if let Some(r) = w.resets_at_ms {
            if r.0 > now_ms {
                out += &format!(" resets in {}", pt_quota::fmt_age(r.0 - now_ms));
            } else {
                out += " reset passed";
            }
        }
        out += "]";
    }
    if !q.note.is_empty() {
        out += &format!(" note={}", q.note);
    }
    out
}
