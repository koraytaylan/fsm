//! The honest security boundary: loopback, `Origin`, and a bearer token.
//!
//! There is no TLS in this binary and there will not be one, so the posture
//! is the one a zero-dependency program can actually deliver — and the
//! defaults are the safe ones, because most people never change a default.
//! Anything beyond loopback is an operator's decision made behind a proxy
//! that terminates TLS, and the flag that allows it says so in its own help
//! text.
//!
//! Plan 0015 tasks 7101 and 7102.

use std::net::{IpAddr, SocketAddr};

/// The origins a server answers before anybody configures one.
pub const DEFAULT_ORIGINS: &[&str] = &["http://localhost", "http://127.0.0.1", "http://[::1]"];

/// What `--http-allow-remote` costs, in the words an operator reads.
pub const REMOTE_HELP: &str = "bind beyond loopback. This binary has no TLS: put it behind a reverse proxy that terminates TLS, or the traffic is in the clear.";

/// One server's posture.
#[derive(Debug, Clone)]
pub struct Policy {
    pub bind: SocketAddr,
    pub path: String,
    pub allow_remote: bool,
    /// Exact origins, scheme and host and port. No wildcards, ever.
    pub origins: Vec<String>,
    /// The token a client must present, if one is configured.
    pub token: Option<String>,
}

impl Policy {
    /// The posture a set of flags asks for, or why this server will not
    /// start with it.
    pub fn new(
        addr: &str,
        path: &str,
        allow_remote: bool,
        extra_origins: &[String],
        token: Option<String>,
    ) -> Result<Self, String> {
        let bind = parse_bind(addr)?;
        // The default is the safe one, and the unsafe one has to be asked
        // for by name.
        if !bind.ip().is_loopback() && !allow_remote {
            return Err(format!(
                "{} is not a loopback address; pass --http-allow-remote to bind it. {REMOTE_HELP}",
                bind.ip()
            ));
        }
        // A token is not optional off loopback. A warning is something a
        // person scrolls past; a refusal is not.
        if !bind.ip().is_loopback() && token.is_none() {
            return Err(
                "a non-loopback bind requires a token: set FSM_HTTP_TOKEN or pass --http-token-file"
                    .to_string(),
            );
        }
        if let Some(token) = &token
            && token.is_empty()
        {
            return Err("the configured token is empty".to_string());
        }
        let mut origins: Vec<String> = DEFAULT_ORIGINS
            .iter()
            .flat_map(|origin| {
                // Loopback origins with and without the port being bound:
                // a browser sends the port it used.
                vec![(*origin).to_string(), format!("{origin}:{}", bind.port())]
            })
            .collect();
        for origin in extra_origins {
            let origin = origin.trim();
            if !origin.is_empty() {
                origins.push(normalise(origin));
            }
        }
        Ok(Self {
            bind,
            path: path.to_string(),
            allow_remote,
            origins,
            token,
        })
    }

    /// The line an operator reads at startup, so the posture is visible
    /// without re-reading the command line they typed.
    pub fn startup_line(&self) -> String {
        format!(
            "fsm http: bind={} path={} remote={} origins=[{}] auth={}",
            self.bind,
            self.path,
            if self.allow_remote {
                "allowed"
            } else {
                "loopback-only"
            },
            self.origins.join(" "),
            match (&self.token, self.bind.ip().is_loopback()) {
                (Some(_), _) => "bearer",
                // Said plainly rather than left to be discovered.
                (None, true) => "none (loopback only)",
                (None, false) => "none",
            }
        )
    }
}

/// `8080`, `127.0.0.1:8080`, or `[::1]:8080`. A bare port binds loopback,
/// because a bare port is what somebody types when they have not thought
/// about the network.
fn parse_bind(addr: &str) -> Result<SocketAddr, String> {
    if let Ok(port) = addr.parse::<u16>() {
        return Ok(SocketAddr::from((IpAddr::from([127, 0, 0, 1]), port)));
    }
    addr.parse::<SocketAddr>()
        .map_err(|_| format!("{addr} is not an address or a port"))
}

/// Scheme and host lowercased; nothing else touched.
fn normalise(origin: &str) -> String {
    match origin.split_once("://") {
        Some((scheme, rest)) => format!("{}://{}", scheme.to_ascii_lowercase(), lower_host(rest)),
        None => origin.to_ascii_lowercase(),
    }
}

/// Lowercase the host and leave the port alone.
fn lower_host(rest: &str) -> String {
    match rest.rsplit_once(':') {
        // An IPv6 literal's colons are inside the brackets.
        Some((host, port)) if !host.ends_with(']') || port.chars().all(|c| c.is_ascii_digit()) => {
            format!("{}:{}", host.to_ascii_lowercase(), port)
        }
        _ => rest.to_ascii_lowercase(),
    }
}

/// Whether an `Origin` header is one this server answers.
///
/// Compared **exactly** — scheme, host and port — with no wildcards, no
/// suffix matching, and no normalisation beyond lowercasing the scheme and
/// the host. A wildcard allow-list is the flaw this check exists to prevent,
/// and a suffix match is a wildcard wearing a disguise: `evil-localhost` ends
/// in `localhost`.
///
/// A **missing** `Origin` is refused too. This is the DNS-rebinding defence
/// the specification requires and it is not optional in any configuration,
/// loopback included.
pub fn origin_allowed(origin: Option<&str>, allowed: &[String]) -> bool {
    let Some(origin) = origin else {
        return false;
    };
    let origin = normalise(origin);
    allowed.contains(&origin)
}

/// Compare a presented token with the configured one in constant time.
///
/// Every byte of both is accumulated into one difference and compared once
/// at the end: no early return on the first mismatch, and no length-based
/// short-circuit before the accumulation. A comparison that returns early
/// tells an attacker how much of their guess was right.
pub fn token_matches(presented: &str, configured: &str) -> bool {
    let presented = presented.as_bytes();
    let configured = configured.as_bytes();
    let mut difference = presented.len() ^ configured.len();
    let width = presented.len().max(configured.len());
    for index in 0..width {
        let a = presented.get(index).copied().unwrap_or(0);
        let b = configured.get(index).copied().unwrap_or(0);
        difference |= usize::from(a ^ b);
    }
    difference == 0
}

/// The token a request presented, if it presented one in the right shape.
///
/// The scheme is matched case-insensitively; the token exactly. A token
/// containing whitespace or a control byte is not a token.
pub fn presented_token(authorization: Option<&str>) -> Option<&str> {
    let value = authorization?;
    let (scheme, token) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim_start_matches(' ');
    if token.is_empty()
        || token
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
    {
        return None;
    }
    Some(token)
}

/// The token on disk, with exactly one trailing newline removed.
///
/// One newline and nothing else: a token is bytes, and trimming whitespace
/// could silently accept a different secret than the one in the file.
pub fn token_from_file(path: &std::path::Path) -> Result<String, String> {
    let bytes =
        std::fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let mut text =
        String::from_utf8(bytes).map_err(|_| "the token file is not UTF-8".to_string())?;
    if text.ends_with('\n') {
        text.pop();
        if text.ends_with('\r') {
            text.pop();
        }
    }
    if text.is_empty() {
        return Err(format!("{} is empty", path.display()));
    }
    Ok(text)
}

/// The token from the environment, if one is set there.
pub fn token_from_env() -> Option<String> {
    std::env::var("FSM_HTTP_TOKEN")
        .ok()
        .filter(|token| !token.is_empty())
}
