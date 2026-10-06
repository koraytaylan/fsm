//! Checked original handler material shared by independent native fixture axes.

use super::*;

pub(super) fn handler_table(timeout: bool) -> Value {
    let mut table = parse(br#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","argv":["/bin/true"],"timeout_ms":1000,"on_ok":{"event":"docs_ok"},"on_failed":{"event":"note_added","payload":{"text":"original"}},"retry":{"attempts":1,"backoff_ms":10,"max_backoff_ms":10,"on":[]}}]}"#, &JsonLimits::DEFAULT).unwrap();
    if timeout {
        let Value::Obj(document) = &mut table else {
            unreachable!()
        };
        let Value::Arr(handlers) = document.get_mut("handlers").unwrap() else {
            unreachable!()
        };
        let Value::Obj(handler) = &mut handlers[0] else {
            unreachable!()
        };
        handler.insert(
            "argv".into(),
            Value::Arr(vec![
                Value::Str("/bin/sleep".into()),
                Value::Str("300".into()),
            ]),
        );
    }
    table
}
