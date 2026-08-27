//! The second transport: Streamable HTTP, hand-rolled over `std::net`.
//!
//! Every module this plan adds is declared here, and each lands as a shell
//! first, because a module cannot be declared without its file — the same
//! move plan 0008's crate scaffold made, and for the same reason: each later
//! task then stays inside one file.
//!
//! The posture is the workspace's own. Blocking threads, no async runtime,
//! no dependencies, and every bound a stranger can reach stated as a
//! constant. There is no TLS here and there will not be one: the server
//! binds loopback by default and anything else is an operator's decision
//! made behind a proxy that terminates TLS.
//!
//! Plan 0015.

pub mod endpoint;
pub mod request;
pub mod response;
pub mod security;
pub mod server;
pub mod session;
pub mod sse;
pub mod writer;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::args::{Args, Ctx};
use crate::mcp::serve::ServeMode;

/// Run the server over HTTP instead of stdio.
///
/// The posture is decided before anything is bound, and printed before
/// anything is served: an operator sees what they are running without
/// re-reading the command line they typed.
pub fn run_http(ctx: &mut Ctx, args: &Args, addr: &str, mode: ServeMode) -> u8 {
    let origins: Vec<String> = args
        .flags
        .get("http-origin")
        .map(|list| {
            list.split(',')
                .map(|origin| origin.trim().to_string())
                .collect()
        })
        .unwrap_or_default();
    let token = match args.flags.get("http-token-file") {
        Some(path) => match security::token_from_file(std::path::Path::new(path)) {
            Ok(token) => Some(token),
            Err(why) => {
                let _ = std::io::Write::write_all(
                    &mut std::io::stderr(),
                    format!("fsm http: {why}\n").as_bytes(),
                );
                return 1;
            }
        },
        None => security::token_from_env(),
    };
    let policy = match security::Policy::new(
        addr,
        args.flags
            .get("http-path")
            .map(String::as_str)
            .unwrap_or(endpoint::DEFAULT_PATH),
        args.switches.contains("http-allow-remote"),
        &origins,
        token,
    ) {
        Ok(policy) => policy,
        Err(why) => {
            // A refusal, not a warning: a warning is something a person
            // scrolls past.
            let _ = std::io::Write::write_all(
                &mut std::io::stderr(),
                format!("fsm http: {why}\n").as_bytes(),
            );
            return 1;
        }
    };
    let _ = std::io::Write::write_all(
        &mut std::io::stderr(),
        format!("{}\n", policy.startup_line()).as_bytes(),
    );

    let store = match mode {
        ServeMode::ReadOnly => crate::store::Store::open_read_only(&ctx.data_dir).ok(),
        _ => crate::store::Store::open(&ctx.data_dir).ok(),
    };
    let bind = policy.bind;
    let endpoint = Arc::new(endpoint::Endpoint::new(&policy.path, store, "").with_policy(policy));
    let handler: Arc<dyn server::Handler> = Arc::new(endpoint::EndpointHandler::new(endpoint));
    match server::serve_http(bind, handler, Arc::new(AtomicBool::new(false))) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}
