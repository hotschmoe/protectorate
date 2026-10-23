//! Grok subscription usage from the Grok CLI's credentials and the CLI chat
//! proxy billing endpoint, plus the read-only remaining-resets listing.
use crate::http::{do_raw, get_json};
use crate::{
    clamp_percent, home_dir, parse_time, read_cred_file, sort_by_expiry, Ctx, Error, Reader, Reset,
    Result, Snapshot, Transport, Window,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

const DEFAULT_PROXY: &str = "https://cli-chat-proxy.grok.com";
const DEFAULT_WEB: &str = "https://grok.com";

#[derive(Clone, Debug)]
pub struct Grok {
    pub home: Option<PathBuf>,
    pub base_url: String,
    pub web_url: String,
    /// Skips the remaining-resets call when true.
    pub no_resets: bool,
}

impl Default for Grok {
    fn default() -> Self {
        Self {
            home: None,
            base_url: DEFAULT_PROXY.to_string(),
            web_url: DEFAULT_WEB.to_string(),
            no_resets: false,
        }
    }
}

impl Grok {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_home(mut self, home: PathBuf) -> Self {
        self.home = Some(home);
        self
    }
    pub fn with_base_url(mut self, base: &str) -> Self {
        self.base_url = base.trim_end_matches('/').to_string();
        self
    }
    pub fn with_web_url(mut self, web: &str) -> Self {
        self.web_url = web.trim_end_matches('/').to_string();
        self
    }
    pub fn with_no_resets(mut self) -> Self {
        self.no_resets = true;
        self
    }
    fn proxy(&self) -> String {
        trim(&self.base_url, DEFAULT_PROXY)
    }
    fn web(&self) -> String {
        trim(&self.web_url, DEFAULT_WEB)
    }
}

fn trim(url: &str, default: &str) -> String {
    let t = url.trim_end_matches('/');
    if t.is_empty() {
        default.to_string()
    } else {
        t.to_string()
    }
}

#[derive(Clone, Debug)]
pub struct GrokEntry {
    key: String,
    expires_at: String,
}

/// Picks the login entry the Grok CLI wrote, preferring the auth.x.ai account.
pub fn pick_entry(raw: &[u8]) -> Result<GrokEntry> {
    let all: BTreeMap<String, Value> = serde_json::from_slice(raw)
        .map_err(|e| Error::Other(format!("decode credentials: {e}")))?;
    for prefix in ["https://auth.x.ai::", "https://accounts.x.ai/sign-in"] {
        for (k, v) in &all {
            if !k.starts_with(prefix) {
                continue;
            }
            if let Ok(e) = serde_json::from_value::<GrokAuthEntry>(v.clone()) {
                if !e.key.is_empty() {
                    return Ok(GrokEntry {
                        key: e.key,
                        expires_at: e.expires_at,
                    });
                }
            }
        }
    }
    Err(Error::unavailable("no Grok login entry"))
}

#[derive(Deserialize)]
struct GrokAuthEntry {
    #[serde(default)]
    key: String,
    #[serde(rename = "expires_at", default)]
    expires_at: String,
}

#[derive(Deserialize, Default)]
struct GrokAmount {
    #[serde(default)]
    val: Value,
}

impl GrokAmount {
    fn float(&self) -> Option<f64> {
        match &self.val {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.parse::<f64>().ok().filter(|v| v.is_finite()),
            _ => None,
        }
    }
}

#[derive(Deserialize, Default)]
struct GrokBilling {
    #[serde(rename = "subscriptionTier", default)]
    subscription_tier: String,
    #[serde(default)]
    config: Option<GrokConfig>,
}

#[derive(Deserialize, Default)]
struct GrokConfig {
    #[serde(rename = "creditUsagePercent", default)]
    credit_usage_percent: Option<f64>,
    #[serde(rename = "currentPeriod", default)]
    current_period: Option<GrokPeriod>,
    #[serde(rename = "billingPeriodStart", default)]
    billing_period_start: String,
    #[serde(rename = "billingPeriodEnd", default)]
    billing_period_end: String,
    #[serde(rename = "onDemandCap", default)]
    on_demand_cap: Option<GrokAmount>,
    #[serde(rename = "onDemandUsed", default)]
    on_demand_used: Option<GrokAmount>,
    #[serde(rename = "subscriptionTier", default)]
    subscription_tier: String,
}

#[derive(Deserialize, Default)]
struct GrokPeriod {
    #[serde(default)]
    start: String,
    #[serde(default)]
    end: String,
}

impl Reader for Grok {
    fn provider(&self) -> &str {
        "grok"
    }

    fn read(&self, ctx: &Ctx, http: &dyn Transport) -> Result<Snapshot> {
        let mut snap = Snapshot {
            provider: "grok".to_string(),
            source: "cli-proxy-billing".to_string(),
            ..Snapshot::default()
        };
        let home = home_dir(self.home.as_deref())?;
        let raw = read_cred_file(&home.join(".grok").join("auth.json"))?;
        let entry = pick_entry(&raw)?;
        if let Some(exp) = parse_time(&entry.expires_at) {
            if ctx.now >= exp {
                return Err(Error::Expired);
            }
        }

        let headers = grok_headers(&entry.key);
        let url = self.proxy() + "/v1/billing?format=credits";
        let b: GrokBilling = get_json(http, ctx, &url, headers.clone())?;
        snap.fetched_at = Some(ctx.now);
        snap.plan = b.subscription_tier.clone();

        if let Some(cfg) = b.config.as_ref() {
            if snap.plan.is_empty() {
                snap.plan = cfg.subscription_tier.clone();
            }
            let mut start = parse_time(&cfg.billing_period_start);
            let mut end = parse_time(&cfg.billing_period_end);
            if let Some(cp) = cfg.current_period.as_ref() {
                if let Some(e) = parse_time(&cp.end) {
                    start = parse_time(&cp.start);
                    end = Some(e);
                }
            }
            let pct = cfg.credit_usage_percent.or_else(|| {
                match (cfg.on_demand_used.as_ref(), cfg.on_demand_cap.as_ref()) {
                    (Some(used), Some(cap)) => match (used.float(), cap.float()) {
                        (Some(u), Some(c)) if c > 0.0 => Some(u / c * 100.0),
                        _ => None,
                    },
                    _ => None,
                }
            });
            // "NaN" and "inf" parse as floats, and a tiny cap overflows the ratio.
            if let Some(pct) = pct.filter(|p| p.is_finite()) {
                snap.windows.push(Window {
                    name: window_name(start, end),
                    used_percent: clamp_percent(pct),
                    resets_at: end,
                });
            }
        }

        if snap.plan.is_empty() {
            let url = self.proxy() + "/v1/settings";
            #[derive(Deserialize)]
            struct Settings {
                #[serde(rename = "subscription_tier_display", default)]
                tier: String,
            }
            if let Ok(s) = get_json::<Settings>(http, ctx, &url, headers.clone()) {
                snap.plan = s.tier;
            }
        }
        if !self.no_resets {
            snap.resets = self.remaining_resets(http, ctx, &entry.key);
        }
        Ok(snap)
    }
}

fn grok_headers(token: &str) -> Vec<(String, String)> {
    vec![
        ("Authorization".to_string(), format!("Bearer {token}")),
        ("x-xai-token-auth".to_string(), "xai-grok-cli".to_string()),
        ("Accept".to_string(), "application/json".to_string()),
    ]
}

fn window_name(start: Option<DateTime<Utc>>, end: Option<DateTime<Utc>>) -> String {
    let (Some(s), Some(e)) = (start, end) else {
        return "credits".to_string();
    };
    if e <= s {
        return "credits".to_string();
    }
    let days = (e - s).num_hours() as f64 / 24.0;
    if (6.0..=8.0).contains(&days) {
        "weekly".to_string()
    } else if (27.0..=32.0).contains(&days) {
        "monthly".to_string()
    } else {
        "credits".to_string()
    }
}

/// Calls the read-only GetRemainingResets listing. Failures yield no resets
/// rather than failing the usage read. Nothing is redeemed.
fn remaining_resets_call(
    http: &dyn Transport,
    ctx: &Ctx,
    web: &str,
    token: &str,
) -> Option<Vec<Reset>> {
    let data = do_raw(
        http,
        ctx,
        "POST",
        &format!("{web}/prod_mc_billing.ConsumerUiSvc/GetRemainingResets"),
        vec![
            ("Authorization".to_string(), format!("Bearer {token}")),
            ("Origin".to_string(), DEFAULT_WEB.to_string()),
            ("Accept".to_string(), "*/*".to_string()),
            (
                "Content-Type".to_string(),
                "application/grpc-web+proto".to_string(),
            ),
            ("x-grpc-web".to_string(), "1".to_string()),
        ],
        Some(vec![0, 0, 0, 0, 0]),
    )
    .ok()?;
    Some(parse_resets(&data, ctx.now))
}

impl Grok {
    fn remaining_resets(&self, http: &dyn Transport, ctx: &Ctx, token: &str) -> Vec<Reset> {
        remaining_resets_call(http, ctx, &self.web(), token).unwrap_or_default()
    }
}

/// grpc-web frames that are data (flag 0), not trailers.
fn grpc_web_data_frames(b: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut b = b;
    while b.len() >= 5 {
        let flag = b[0];
        let len = u32::from_be_bytes([b[1], b[2], b[3], b[4]]) as usize;
        if flag & 0x7f != 0 || len > b.len() - 5 {
            return Vec::new();
        }
        if flag == 0 && len > 0 {
            out.push(&b[5..5 + len]);
        }
        b = &b[5 + len..];
    }
    out
}

struct PbField {
    num: u64,
    varint: u64,
    data: Option<Vec<u8>>,
}

fn uvarint(b: &[u8]) -> Option<(u64, usize)> {
    let mut v: u64 = 0;
    let mut shift = 0;
    for (i, byte) in b.iter().enumerate() {
        v |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some((v, i + 1));
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
    None
}

/// A tolerant protobuf wire-format parser: enough to walk unknown messages.
fn pb_parse(b: &[u8]) -> Option<Vec<PbField>> {
    let mut fs = Vec::new();
    let mut b = b;
    while !b.is_empty() {
        let (tag, n) = uvarint(b)?;
        b = &b[n..];
        let mut f = PbField {
            num: tag >> 3,
            varint: 0,
            data: None,
        };
        match tag & 7 {
            0 => {
                let (v, n) = uvarint(b)?;
                f.varint = v;
                b = &b[n..];
            }
            1 => {
                if b.len() < 8 {
                    return None;
                }
                b = &b[8..];
            }
            2 => {
                let (len, n) = uvarint(b)?;
                let rest = &b[n..];
                if len as usize > rest.len() {
                    return None;
                }
                f.data = Some(rest[..len as usize].to_vec());
                b = &rest[len as usize..];
            }
            5 => {
                if b.len() < 4 {
                    return None;
                }
                b = &b[4..];
            }
            _ => return None,
        }
        fs.push(f);
    }
    Some(fs)
}

/// Extracts expiry times from the record layout CodexBar documents: field 10 =
/// reset record { 10 = id, 30 = Timestamp { 1 = seconds } }.
pub fn parse_resets(data: &[u8], now: DateTime<Utc>) -> Vec<Reset> {
    let frames = grpc_web_data_frames(data);
    let payloads: Vec<&[u8]> = if frames.is_empty() {
        vec![data]
    } else {
        frames
    };
    let mut out: Vec<Reset> = Vec::new();
    for p in payloads {
        let Some(top) = pb_parse(p) else { continue };
        for f in &top {
            if f.num != 10 {
                continue;
            }
            let Some(rec_bytes) = f.data.as_ref() else {
                continue;
            };
            let Some(rec) = pb_parse(rec_bytes) else {
                continue;
            };
            for rf in &rec {
                if rf.num != 30 {
                    continue;
                }
                let Some(ts_bytes) = rf.data.as_ref() else {
                    continue;
                };
                let Some(ts) = pb_parse(ts_bytes) else {
                    continue;
                };
                for tf in &ts {
                    if tf.num == 1
                        && tf.data.is_none()
                        && (1_700_000_000..=2_100_000_000).contains(&tf.varint)
                    {
                        let exp = chrono::DateTime::from_timestamp(tf.varint as i64, 0);
                        if let Some(exp) = exp {
                            if exp > now {
                                out.push(Reset {
                                    kind: "usage-limit".to_string(),
                                    count: 1,
                                    expires_at: Some(exp),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    sort_by_expiry(&mut out, |r| r.expires_at);
    out
}
