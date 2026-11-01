//! The GitHub releases API and asset downloads, through the system curl.

use crate::{Channel, Error, ErrorKind, Result, Version};
use pt_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

pub(crate) const MAX_LISTING: u64 = 16 << 20;
pub(crate) const MAX_SUMS: u64 = 1 << 20;
pub(crate) const MAX_VERSION: u64 = 1024;
pub(crate) const MAX_ASSET: u64 = 1 << 30;
const USER_AGENT: &str = "protectorate-update";

/// Where releases come from: an API base (`https://api.github.com`, or a loopback `http://` URL
/// for tests) and an `owner/name` repo.
#[derive(Debug, Clone)]
pub struct Source {
    api: String,
    repo: String,
    curl: PathBuf,
    allow_http: bool,
    connect_timeout: Duration,
    api_timeout: Duration,
    asset_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    /// The size GitHub reports; 0 when absent.
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}

fn rel(msg: impl Into<String>) -> Error {
    Error::new(ErrorKind::Release, msg)
}

fn fetch_err(msg: impl Into<String>) -> Error {
    Error::new(ErrorKind::Fetch, msg)
}

fn loopback_host(authority: &str) -> bool {
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split_once(']').map_or("", |(h, _)| h)
    } else {
        authority.split(':').next().unwrap_or("")
    };
    matches!(host, "127.0.0.1" | "::1" | "localhost")
}

fn repo_ok(repo: &str) -> bool {
    let part = |s: &str| {
        !s.is_empty()
            && s != "."
            && s != ".."
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    };
    repo.split_once('/')
        .is_some_and(|(o, n)| part(o) && part(n))
}

impl Release {
    pub fn version(&self) -> Option<Version> {
        Version::parse(&self.tag)
    }

    /// The one asset called `name`.
    pub fn asset(&self, name: &str) -> Result<&Asset> {
        let mut found = self.assets.iter().filter(|a| a.name == name);
        let a = found
            .next()
            .ok_or_else(|| rel(format!("release {} has no asset {name}", self.tag)))?;
        if found.next().is_some() {
            return Err(rel(format!("release {} lists {name} twice", self.tag)));
        }
        Ok(a)
    }

    fn from_json(v: &Value) -> Result<Release> {
        let tag = v["tag_name"]
            .as_str()
            .ok_or_else(|| rel("release without tag_name"))?
            .to_string();
        let mut assets = Vec::new();
        for a in v["assets"].as_array().unwrap_or(&[]) {
            let (Some(name), Some(url)) = (a["name"].as_str(), a["browser_download_url"].as_str())
            else {
                return Err(rel(format!("release {tag}: asset without name or URL")));
            };
            assets.push(Asset {
                name: name.to_string(),
                url: url.to_string(),
                size: a["size"].as_u64().unwrap_or(0),
            });
        }
        Ok(Release {
            draft: v["draft"].as_bool().unwrap_or(false),
            prerelease: v["prerelease"].as_bool().unwrap_or(false),
            tag,
            assets,
        })
    }
}

impl Source {
    pub fn github(repo: &str) -> Result<Source> {
        Source::new("https://api.github.com", repo)
    }

    /// `api` must be `https://`, or `http://` to a loopback host (a local fake release).
    pub fn new(api: &str, repo: &str) -> Result<Source> {
        if !repo_ok(repo) {
            return Err(rel(format!("release repo {repo:?} is not OWNER/NAME")));
        }
        let api = api.trim_end_matches('/');
        let allow_http = if api.starts_with("https://") {
            false
        } else if let Some(rest) = api.strip_prefix("http://") {
            if !loopback_host(rest.split('/').next().unwrap_or("")) {
                return Err(rel(format!("{api}: plain http only to a loopback host")));
            }
            true
        } else {
            return Err(rel(format!("{api}: not an https:// URL")));
        };
        Ok(Source {
            api: api.to_string(),
            repo: repo.to_string(),
            curl: PathBuf::from("curl"),
            allow_http,
            connect_timeout: Duration::from_secs(15),
            api_timeout: Duration::from_secs(60),
            asset_timeout: Duration::from_secs(30 * 60),
        })
    }

    pub fn with_curl(mut self, curl: impl Into<PathBuf>) -> Source {
        self.curl = curl.into();
        self
    }

    pub fn repo(&self) -> &str {
        &self.repo
    }

    /// The newest releases (up to 100), drafts included as GitHub lists them.
    pub fn releases(&self) -> Result<Vec<Release>> {
        let url = format!("{}/repos/{}/releases?per_page=100", self.api, self.repo);
        let body = self.get(&url, MAX_LISTING, true)?;
        let v = pt_json::parse_bytes(&body).map_err(|e| rel(format!("{url}: {e}")))?;
        let list = v
            .as_array()
            .ok_or_else(|| rel(format!("{url}: not a JSON array")))?;
        list.iter().map(Release::from_json).collect()
    }

    pub fn release(&self, tag: &str) -> Result<Release> {
        if !crate::store::version_name(tag) {
            return Err(rel(format!("{tag:?} is not a release tag")));
        }
        let url = format!("{}/repos/{}/releases/tags/{tag}", self.api, self.repo);
        let body = self.get(&url, MAX_LISTING, true)?;
        let v = pt_json::parse_bytes(&body).map_err(|e| rel(format!("{url}: {e}")))?;
        let r = Release::from_json(&v).map_err(|e| e.context(&url))?;
        if r.tag != tag {
            return Err(rel(format!("{url}: answered with release {}", r.tag)));
        }
        Ok(r)
    }

    /// The newest non-draft release the channel admits, by tag precedence. Tags that are not
    /// `vX.Y.Z[-PRE]` are ignored.
    pub fn latest(&self, channel: Channel) -> Result<Release> {
        self.releases()?
            .into_iter()
            .filter(|r| !r.draft)
            .filter_map(|r| Some((r.version()?, r)))
            .filter(|(v, _)| channel.admits(v))
            .max_by(|a, b| a.0.cmp(&b.0))
            .map(|(_, r)| r)
            .ok_or_else(|| rel(format!("{} has no {} release", self.repo, channel.as_str())))
    }

    fn curl(&self, url: &str, timeout: Duration, max: u64) -> Result<Command> {
        let ok = url.starts_with("https://")
            || (self.allow_http
                && url
                    .strip_prefix("http://")
                    .is_some_and(|r| loopback_host(r.split('/').next().unwrap_or(""))));
        if !ok {
            return Err(rel(format!("refusing to fetch {url}")));
        }
        let mut c = Command::new(&self.curl);
        c.args(["--fail", "--silent", "--show-error", "--location"])
            .args(["--max-redirs", "10", "--proto-redir", "=https"])
            .args([
                "--proto",
                if self.allow_http {
                    "=http,https"
                } else {
                    "=https"
                },
            ])
            .arg("--connect-timeout")
            .arg(self.connect_timeout.as_secs().to_string())
            .arg("--max-time")
            .arg(timeout.as_secs().max(1).to_string())
            .arg("--max-filesize")
            .arg(max.to_string())
            .args(["--user-agent", USER_AGENT])
            .stdin(Stdio::null());
        Ok(c)
    }

    fn run(&self, mut c: Command, url: &str) -> Result<Vec<u8>> {
        let out = c
            .output()
            .map_err(|e| fetch_err(format!("running {} for {url}: {e}", self.curl.display())))?;
        if !out.status.success() {
            return Err(fetch_err(format!(
                "curl {url}: {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(out.stdout)
    }

    /// A body into memory, at most `max` bytes.
    pub(crate) fn get(&self, url: &str, max: u64, api: bool) -> Result<Vec<u8>> {
        let mut c = self.curl(url, self.api_timeout, max)?;
        if api {
            c.args(["--header", "Accept: application/vnd.github+json"])
                .args(["--header", "X-GitHub-Api-Version: 2022-11-28"]);
        }
        c.arg(url);
        let body = self.run(c, url)?;
        if body.len() as u64 > max {
            return Err(fetch_err(format!("{url}: body over {max} bytes")));
        }
        Ok(body)
    }

    /// An asset into `dest` (created or replaced), at most `max` bytes. Returns its length.
    pub(crate) fn download(&self, url: &str, max: u64, dest: &Path) -> Result<u64> {
        let mut c = self.curl(url, self.asset_timeout, max)?;
        c.arg("--output").arg(dest).arg(url);
        self.run(c, url)?;
        let len = std::fs::metadata(dest)
            .map_err(|e| Error::io(format!("reading {}", dest.display()), e))?
            .len();
        if len > max {
            return Err(fetch_err(format!("{url}: body over {max} bytes")));
        }
        Ok(len)
    }
}
