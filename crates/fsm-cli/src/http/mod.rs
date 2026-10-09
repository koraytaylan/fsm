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
mod mailbox;
pub mod request;
pub mod response;
pub mod security;
pub mod server;
pub mod session;
pub mod sse;
mod startup;
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
    #[cfg(not(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )))]
    if matches!(&mode, ServeMode::Embedded(_)) {
        let _ = std::io::Write::write_all(
            &mut std::io::stderr(),
            b"fsm http: native embedded HTTP is unsupported on this platform\n",
        );
        return 1;
    }
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

    let opened = startup::open(&ctx.data_dir, &mode);
    let mut store = opened.store;
    let bind = match server::bind(policy.bind) {
        Ok(bind) => bind,
        Err(_) => return 1,
    };
    let stop = Arc::new(AtomicBool::new(false));
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    let mut native = None;
    let hosted_writer = if store
        .as_ref()
        .is_some_and(|store| !store.journal.is_read_only())
    {
        match mode {
            ServeMode::Writer => match crate::mcp::http_host::SharedWriter::start(
                store.take().expect("observed original writer"),
            ) {
                Ok(host) => Some(Arc::new(host)),
                Err(_) => return 1,
            },
            #[cfg(all(
                target_os = "linux",
                any(target_arch = "x86_64", target_arch = "aarch64")
            ))]
            ServeMode::Embedded(executor) => {
                match crate::mcp::http_host::native::NativeServer::start(
                    store.take().expect("observed original writer"),
                    *executor,
                    Arc::clone(&stop),
                ) {
                    Ok((host, lifetime)) => {
                        native = Some(lifetime);
                        Some(host)
                    }
                    Err(error) => {
                        let _ = std::io::Write::write_all(
                            &mut std::io::stderr(),
                            format!("fsm http: native startup failed: {error}\n").as_bytes(),
                        );
                        return 1;
                    }
                }
            }
            _ => None,
        }
    } else {
        None
    };
    // One flag for both halves: the accept loop stops taking connections and
    // the streams parked in `Endpoint::deliver` end, rather than holding
    // threads open past the server they belong to.
    let mut endpoint = endpoint::Endpoint::new(&policy.path, store, opened.mode_note);
    if let Some(host) = hosted_writer {
        endpoint = endpoint.with_host(host);
    }
    if let Some(detail) = opened.diagnostic {
        endpoint = endpoint.with_degraded(ctx.data_dir.clone(), detail);
    }
    let endpoint = Arc::new(endpoint.with_policy(policy).with_stop(Arc::clone(&stop)));
    let handler: Arc<dyn server::Handler> = Arc::new(endpoint::EndpointHandler::new(endpoint));
    let served = server::serve_bound(bind, handler, stop);
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    if let Some(native) = &mut native
        && let Err(error) = native.finish()
    {
        if let Some(failure) = error
            .get_ref()
            .and_then(|error| error.downcast_ref::<crate::native_error::NativeSessionFailure>())
        {
            return crate::native_error::report_until(ctx, failure);
        }
        return 1;
    }
    match served {
        Ok(()) => 0,
        Err(_) => 1,
    }
}
