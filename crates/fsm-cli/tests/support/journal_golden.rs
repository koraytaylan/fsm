//! Optional raw inputs for independent format-change golden verification.

pub fn capture(name: &str, records: &[fsm_core::record::Record]) {
    let Some(directory) = std::env::var_os("FSM_GOLDEN_JOURNAL_CAPTURE") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).expect("capture directory is writable");
    let bytes: Vec<u8> = records
        .iter()
        .flat_map(fsm_core::record::Record::to_line)
        .collect();
    std::fs::write(directory.join(format!("{name}.jsonl")), bytes)
        .expect("raw golden inputs are writable");
}
