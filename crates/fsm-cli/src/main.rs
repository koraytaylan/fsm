fn main() -> std::process::ExitCode {
    fsm_cli::args::dispatch(std::env::args().collect()).into()
}
