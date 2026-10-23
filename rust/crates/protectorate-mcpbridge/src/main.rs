//! Thin binary over the bridge: flags and hub environment in, JSON-RPC on
//! stdin/stdout, or on a bearer-authenticated streamable HTTP(S) endpoint with
//! `--http` or `--https`. `--list` prints the catalog the bridge would serve and exits
//! without contacting the hub.
use protectorate_mcpbridge::{
    bind, bind_tls, serve, serve_tls, tls_config, Bridge, Config, Credential, Filter, HttpConfig,
    Hub, DEFAULT_HUB, DEFAULT_TIMEOUT,
};
use protectorate_mcpbridge::{Code, Result as HubResult, RpcError};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const USAGE: &str = r#"usage: protectorate-mcpbridge [--allow GLOB] [--deny GLOB] [--list]
                  [--hub URL] [--token-file PATH] [--timeout SECONDS]
                  [--http ADDR | --https ADDR --tls-cert FILE --tls-key FILE] [--help]

Serves every unary hub RPC as an MCP tool over stdio, loopback streamable HTTP,
or HTTPS on an explicit private address. Network requests require the bridge token as
a bearer. Without --allow only read-only tools are served; --allow replaces
that default with globs over tool names and --deny always removes matches. The
hub URL comes from --hub or PROTECTORATE_HUB. The bridge authenticates with the
capability in --token-file or PROTECTORATE_TOKEN_FILE, read for each call and
renewed in place, a token in PROTECTORATE_TOKEN, or the device key file in
PROTECTORATE_DEVICE_CREDENTIAL; the network endpoints need a token.
"#;

struct Options {
    filter: Filter,
    list: bool,
    hub: Option<String>,
    token_file: Option<PathBuf>,
    timeout: Duration,
    http: Option<String>,
    https: Option<String>,
    tls_cert: Option<PathBuf>,
    tls_key: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        let mut stdout = io::stdout();
        return stdout_result(
            stdout
                .write_all(USAGE.as_bytes())
                .and_then(|()| stdout.flush()),
        );
    }
    match run(args.into_iter()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {}", error.message);
            ExitCode::FAILURE
        }
    }
}

fn run(args: impl Iterator<Item = String>) -> HubResult<()> {
    let options = match parse(args) {
        Ok(options) => options,
        Err(message) => return Err(RpcError::invalid(message)),
    };
    let version = env!("CARGO_PKG_VERSION");
    if options.list {
        let bridge = Bridge::new(Config {
            version: version.to_owned(),
            filter: options.filter,
            hub: None,
        })?;
        let text = serde_json::to_string_pretty(&bridge.tools())
            .map_err(|error| internal(format!("encode tool catalog: {error}")))?;
        let mut stdout = io::stdout();
        match writeln!(stdout, "{text}").and_then(|()| stdout.flush()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
            Err(error) => Err(internal(format!("write tool catalog: {error}"))),
        }
    } else {
        let hub_url = options
            .hub
            .or_else(|| env("PROTECTORATE_HUB"))
            .unwrap_or_else(|| DEFAULT_HUB.to_owned());
        let credential = credential(options.token_file)?;
        let token = match &credential {
            Credential::TokenFile(path) => std::fs::read_to_string(path)
                .map(|raw| raw.trim().to_owned())
                .map_err(|error| internal(format!("read token file: {error}")))?,
            Credential::Bearer(token) => token.to_string(),
            _ => String::new(),
        };
        let bridge = Arc::new(Bridge::new(Config {
            version: version.to_owned(),
            filter: options.filter,
            hub: Some(Hub {
                url: hub_url,
                credential,
                timeout: options.timeout,
            }),
        })?);
        match (&options.http, &options.https) {
            (Some(listen), _) => serve_http(bridge, listen, token),
            (_, Some(listen)) => serve_https(
                bridge,
                listen,
                options.tls_cert.as_ref().expect("validated TLS cert"),
                options.tls_key.as_ref().expect("validated TLS key"),
                token,
            ),
            (None, None) => {
                let output = Arc::new(Mutex::new(io::stdout()));
                bridge
                    .run(io::stdin().lock(), &output)
                    .map_err(|error| internal(format!("stdio exchange failed: {error}")))
            }
        }
    }
}

fn stdout_result(result: io::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: write stdout: {error}");
            ExitCode::FAILURE
        }
    }
}

fn serve_https(
    bridge: Arc<Bridge>,
    listen: &str,
    cert: &std::path::Path,
    key: &std::path::Path,
    token: String,
) -> HubResult<()> {
    let tls = tls_config(cert, key)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| internal(format!("start the HTTPS runtime: {error}")))?;
    let config = HttpConfig::new(token)?;
    runtime.block_on(async move {
        let listener = bind_tls(listen, tls).await?;
        eprintln!("mcp listening on https://{}", listener.local_addr());
        serve_tls(listener, bridge, config, async {
            let _ = tokio::signal::ctrl_c().await;
            eprintln!("mcp stopping");
        })
        .await
    })
}

/// Runs the HTTP endpoint on its own runtime, so stdio mode stays single
/// threaded and never pays for one.
fn serve_http(bridge: Arc<Bridge>, listen: &str, token: String) -> HubResult<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| internal(format!("start the HTTP runtime: {error}")))?;
    let config = HttpConfig::new(token)?;
    runtime.block_on(async move {
        let listener = bind(listen).await?;
        let addr = listener
            .local_addr()
            .map_err(|error| internal(format!("read the listen address: {error}")))?;
        eprintln!("mcp listening on http://{addr}");
        serve(listener, bridge, config, async {
            let _ = tokio::signal::ctrl_c().await;
            eprintln!("mcp stopping");
        })
        .await
    })
}

fn internal(message: String) -> RpcError {
    RpcError::new(Code::Internal, message)
}

fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn credential(file: Option<PathBuf>) -> HubResult<Credential> {
    let file = file.or_else(|| env("PROTECTORATE_TOKEN_FILE").map(PathBuf::from));
    if let Some(file) = file {
        let raw = std::fs::read_to_string(&file)
            .map_err(|error| internal(format!("read token file: {error}")))?;
        if raw.trim().is_empty() {
            return Err(RpcError::invalid(format!(
                "token file {} is empty",
                file.display()
            )));
        }
        return Ok(Credential::TokenFile(file));
    }
    if let Some(token) = env("PROTECTORATE_TOKEN") {
        return Ok(Credential::Bearer(token.into()));
    }
    match env("PROTECTORATE_DEVICE_CREDENTIAL") {
        Some(path) => protectorate_connect_client::read_device_credential(path.as_ref()),
        None => Err(RpcError::new(
            Code::Unauthenticated,
            "no credential: set --token-file, PROTECTORATE_TOKEN_FILE, PROTECTORATE_TOKEN or PROTECTORATE_DEVICE_CREDENTIAL",
        )),
    }
}

fn parse(args: impl Iterator<Item = String>) -> std::result::Result<Options, String> {
    let mut options = Options {
        filter: Filter::default(),
        list: false,
        hub: None,
        token_file: None,
        timeout: DEFAULT_TIMEOUT,
        http: None,
        https: None,
        tls_cert: None,
        tls_key: None,
    };
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) => (flag.to_owned(), Some(value.to_owned())),
            None => (arg.clone(), None),
        };
        let mut value = |name: &str| -> std::result::Result<String, String> {
            if let Some(value) = inline.clone() {
                return Ok(value);
            }
            args.next().ok_or_else(|| format!("{name} needs a value"))
        };
        match flag.as_str() {
            "--allow" => options
                .filter
                .allow
                .extend(slice_value(&value("--allow")?)?),
            "--deny" => options.filter.deny.extend(slice_value(&value("--deny")?)?),
            "--hub" => options.hub = Some(value("--hub")?),
            "--token-file" => options.token_file = Some(PathBuf::from(value("--token-file")?)),
            "--timeout" => {
                let seconds = value("--timeout")?;
                let seconds: u64 = seconds
                    .parse()
                    .map_err(|_| format!("--timeout {seconds:?} is not a number of seconds"))?;
                options.timeout = Duration::from_secs(seconds);
            }
            "--list" => options.list = true,
            "--http" => options.http = Some(value("--http")?),
            "--https" => options.https = Some(value("--https")?),
            "--tls-cert" => options.tls_cert = Some(PathBuf::from(value("--tls-cert")?)),
            "--tls-key" => options.tls_key = Some(PathBuf::from(value("--tls-key")?)),
            "--help" | "-h" => return Err(USAGE.to_owned()),
            other if other.starts_with('-') => return Err(format!("unknown flag {other}")),
            other => return Err(format!("unexpected argument {other:?}")),
        }
    }
    if options.http.is_some() && options.https.is_some() {
        return Err("--http and --https cannot be combined".to_owned());
    }
    if options.https.is_some() && (options.tls_cert.is_none() || options.tls_key.is_none()) {
        return Err("--https requires --tls-cert and --tls-key".to_owned());
    }
    if options.https.is_none() && (options.tls_cert.is_some() || options.tls_key.is_some()) {
        return Err("--tls-cert and --tls-key require --https".to_owned());
    }
    Ok(options)
}

// Cobra's StringSlice flags accept comma-separated values, including quoted
// values with literal commas. Keep repeated flags additive.
fn slice_value(value: &str) -> std::result::Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut field = String::new();
    let mut chars = value.chars().peekable();
    let mut quoted = false;
    let mut after_quote = false;
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' if quoted => {
                quoted = false;
                after_quote = true;
            }
            '"' if field.is_empty() && !after_quote => quoted = true,
            ',' if !quoted => {
                result.push(std::mem::take(&mut field));
                after_quote = false;
            }
            _ if after_quote => return Err(format!("invalid comma-separated pattern {value:?}")),
            _ => field.push(ch),
        }
    }
    if quoted {
        return Err(format!("unterminated quoted pattern {value:?}"));
    }
    result.push(field);
    Ok(result)
}
