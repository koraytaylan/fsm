//! A scriptable stdio MCP server, for exercising the executor's client.
//!
//! **This is a test fixture, declared as a `[[bin]]` because it has to be a
//! real process.** The properties under test are about pipes, exit, and kill,
//! so the server on the other end has to be a separate program — and it cannot
//! be this crate's test binary re-executed, the trick the rest of the suite
//! uses, because libtest writes `running 1 test` to that child's **stdout**
//! before the test body runs. On a protocol stream that banner is a malformed
//! message, which is exactly what the client is supposed to refuse. A `[[bin]]`
//! is the one target whose stdout carries only what it writes.
//!
//! One argument selects the script; each bends a correct exchange in exactly
//! one way. An unknown script answers correctly, echoing the arguments it was
//! called with so a test can prove they arrived substituted.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::io::{BufRead, Write};

const PROTOCOL_VERSION: &str = "2025-06-18";

fn say(line: &str) {
    println!("{line}");
    let _ = std::io::stdout().flush();
}

/// Stay alive without writing, so a run is still in flight when a test kills
/// it. A server that exited would be reaped instead, which is a different row.
fn linger() -> ! {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn main() {
    let script = std::env::args().nth(1).unwrap_or_default();
    match script.as_str() {
        // Never says anything at all: the shape a timeout exists for.
        "silent" => linger(),
        // Gone before the handshake can finish.
        "exit-early" => std::process::exit(7),
        _ => {}
    }

    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let _initialize = lines.next();
    match script.as_str() {
        "no-init-result" => {
            say(r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32603,"message":"no"}}"#);
            linger();
        }
        "malformed" => {
            say("this is not json");
            linger();
        }
        "not-an-object" => {
            say("[1,2,3]");
            linger();
        }
        "oversized" => {
            // Past the client's line cap, with no newline anywhere in it.
            let chunk = "x".repeat(64 * 1024);
            for _ in 0..300 {
                print!("{chunk}");
            }
            let _ = std::io::stdout().flush();
            linger();
        }
        "wrong-id" => {
            say(r#"{"jsonrpc":"2.0","id":99,"result":{}}"#);
            linger();
        }
        "close-after-handshake" => {
            say(&format!(
                r#"{{"jsonrpc":"2.0","id":1,"result":{{"protocolVersion":"{PROTOCOL_VERSION}"}}}}"#
            ));
            std::process::exit(0);
        }
        _ => say(&format!(
            r#"{{"jsonrpc":"2.0","id":1,"result":{{"protocolVersion":"{PROTOCOL_VERSION}","capabilities":{{}},"serverInfo":{{"name":"stub","version":"1"}}}}}}"#
        )),
    }

    let _initialized = lines.next();
    let call = lines.next().and_then(Result::ok).unwrap_or_default();
    match script.as_str() {
        "chatty" => {
            say(
                r#"{"jsonrpc":"2.0","method":"notifications/message","params":{"level":"info","data":"working"}}"#,
            );
            say(r#"{"jsonrpc":"2.0","method":"notifications/progress","params":{"progress":1}}"#);
            // A request of its own, which this client declared no capability
            // to answer and must simply ignore.
            say(r#"{"jsonrpc":"2.0","id":900,"method":"roots/list","params":{}}"#);
            say(r#"{"jsonrpc":"2.0","id":2,"result":{"structuredContent":{"seen":"chatty"}}}"#);
        }
        "call-malformed" => say("}{"),
        "call-wrong-id" => say(r#"{"jsonrpc":"2.0","id":41,"result":{}}"#),
        "call-no-result" => say(r#"{"jsonrpc":"2.0","id":2}"#),
        "call-silent" => linger(),
        "tool-error" => say(
            r#"{"jsonrpc":"2.0","id":2,"result":{"isError":true,"content":[{"type":"text","text":"no reviewer"}]}}"#,
        ),
        "rpc-error" => {
            say(r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32602,"message":"unknown tool"}}"#)
        }
        "content-only" => say(
            r#"{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"summary"}]}}"#,
        ),
        "noisy-stderr" => {
            eprint!("{}", "e".repeat(9000));
            let _ = std::io::stderr().flush();
            say(r#"{"jsonrpc":"2.0","id":2,"result":{"structuredContent":{"ok":true}}}"#);
        }
        // The default answers with the arguments it was given, which is how a
        // test proves the template reached the tool substituted.
        _ => {
            let arguments = call
                .split_once(r#""arguments":"#)
                .map(|(_, rest)| rest.trim_end_matches('}').to_string())
                .unwrap_or_else(|| "{}".to_string());
            say(&format!(
                r#"{{"jsonrpc":"2.0","id":2,"result":{{"structuredContent":{{"echo":{arguments}}}}}}}"#
            ));
        }
    }
    // Alive after answering: the runner stops a server once its conversation
    // is over, and a stub that exited on its own would hide a runner that
    // forgot to.
    linger()
}
