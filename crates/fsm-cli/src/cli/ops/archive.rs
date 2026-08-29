//! `fsm journal archive`, and the sealing vocabulary the audit commands share.
//!
//! Split out of `ops.rs` because sealing is its own subject and `ops.rs` was
//! already at the thousand-line ceiling. What lives here is everything that
//! only exists because a store can be sealed: the operator command, the one
//! report shape its preview and its run both render, the verdict word every
//! surface uses, and the base a sealed store replays from.

use std::collections::BTreeMap;

use fsm_core::json::Value;

use crate::args::{Args, Ctx};
use crate::journal_io::SealVerdict;
use crate::render::{emit_error, emit_success};
use crate::store::ErrorObj;

/// `fsm journal archive --to <dir> [--before-seq N] [--dry-run]`.
///
/// The ordinary form is `--to <dir>` and nothing else: it seals everything up
/// to a cut it creates. `--before-seq` asserts which sequence that will be, so
/// a preview and a run cannot disagree about which prefix moved. There is no
/// confirmation prompt — the command is explicit, `--to` is mandatory,
/// `--dry-run` exists, and nothing is deleted; a prompt here would be the only
/// interactive path in the binary.
pub(super) fn journal_archive(ctx: &mut Ctx, args: &Args) -> u8 {
    let Some(archive_dir) = args.flags.get("to").map(std::path::PathBuf::from) else {
        return emit_error(
            ctx,
            &ErrorObj::new("args", "journal archive requires --to <dir>").hint(concat!(
                "name the directory the sealed segments move to. There is no default: this ",
                "operation relocates history, and a default path is how history ends up ",
                "somewhere nobody looks",
            )),
        );
    };
    let expect_cut = match args.flags.get("before-seq") {
        None => None,
        Some(raw) => match raw.parse::<u64>() {
            Ok(seq) => Some(seq),
            Err(_) => {
                return emit_error(
                    ctx,
                    &ErrorObj::new("args", "before-seq must be a u64")
                        .hint("pass the sequence `--dry-run` reported, or omit the flag"),
                );
            }
        },
    };

    if args.switches.contains("dry-run") {
        // Read-only, so an operator can ask from a monitoring session without
        // taking the writer lock — and so a preview appends no checkpoint and
        // performs no rotation.
        let store = match crate::store::Store::open_read_only(&ctx.data_dir) {
            Ok(store) => store,
            Err(error) => return emit_error(ctx, &error),
        };
        return match store.preview_seal(expect_cut) {
            // A preview that reported a plan the real command will reject is a
            // preview that costs an outage to discover.
            Err(error) => emit_error(ctx, &error),
            Ok(report) => {
                emit_success(ctx, &archive_report(&report, true, &archive_dir));
                0
            }
        };
    }

    let mut store = match crate::store::Store::open(&ctx.data_dir) {
        Ok(store) => store,
        Err(error) => return emit_error(ctx, &error),
    };
    match store.seal_and_archive(&archive_dir, expect_cut) {
        Err(error) => emit_error(ctx, &error),
        Ok(report) => {
            emit_success(ctx, &archive_report(&report, false, &archive_dir));
            0
        }
    }
}

/// One shape for the preview and the run, so the two cannot describe the same
/// operation differently.
pub(super) fn archive_report(
    report: &fsm_store::store::SealReport,
    dry_run: bool,
    archive_dir: &std::path::Path,
) -> Value {
    let mut fields = BTreeMap::from([
        ("ok".into(), Value::Str("true".into())),
        ("dry_run".into(), Value::Bool(dry_run)),
        (
            "archive_dir".into(),
            Value::Str(archive_dir.display().to_string()),
        ),
        (
            "sealed_through_seq".into(),
            Value::Num(report.sealed_through_seq.to_string()),
        ),
        (
            "records_sealed".into(),
            Value::Num(report.records_sealed.to_string()),
        ),
        (
            "segments".into(),
            Value::Arr(
                report
                    .segments
                    .iter()
                    .map(|name| Value::Str(name.clone()))
                    .collect(),
            ),
        ),
        (
            "keys_carried".into(),
            Value::Num(report.keys_carried.to_string()),
        ),
        (
            "keys_dropped".into(),
            Value::Num(report.keys_dropped.to_string()),
        ),
    ]);
    // A preview knows the prefix and the partition; it does not know the hash
    // of a checkpoint that does not exist yet, and reporting one would be a
    // guess.
    if !dry_run {
        fields.insert(
            "sealed_last_hash".into(),
            Value::Str(report.sealed_last_hash.clone()),
        );
        fields.insert("archive_id".into(), Value::Str(report.archive_id.clone()));
        if let Some(seq) = report.seal_record_seq {
            fields.insert("seal_record_seq".into(), Value::Num(seq.to_string()));
        }
    }
    Value::Obj(fields)
}

/// The verdict as the one word every surface uses for it.
pub(super) fn seal_verdict_name(verdict: SealVerdict) -> &'static str {
    match verdict {
        SealVerdict::Unsealed => "unsealed",
        SealVerdict::PrefixNotPresented => "prefix_not_presented",
        SealVerdict::PrefixWalked => "prefix_walked",
        SealVerdict::PrefixNotMatched => "prefix_not_matched",
    }
}

/// The seal a store carries and the base to replay from, or `None` when it is
/// not sealed.
#[allow(clippy::type_complexity)]
pub(super) fn sealed_origin(
    data_dir: &std::path::Path,
    records: &[fsm_core::record::Record],
) -> Result<Option<(fsm_store::base::SealInfo, fsm_core::replay::StoreState)>, ErrorObj> {
    if crate::journal_io::chain_start(data_dir).is_origin() {
        return Ok(None);
    }
    let (base, seal) = fsm_store::base::open_from_base(data_dir, records)?;
    Ok(Some((seal, base)))
}
