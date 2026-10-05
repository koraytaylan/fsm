//! Separately provisioned privileged control entry point, not a handler runner.

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod authority;

fn main() {
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    let result = authority::run(std::env::args_os().skip(1).collect());
    #[cfg(not(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )))]
    let result: Result<(), String> = Err("native containment runtime unsupported".into());
    if let Err(error) = result {
        use std::io::Write;
        let _ = writeln!(std::io::stderr().lock(), "containment authority: {error}");
        std::process::exit(1);
    }
}
