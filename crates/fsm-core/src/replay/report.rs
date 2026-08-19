use std::collections::BTreeMap;

use crate::json::Value;

pub(super) fn enabled_reports_value(evs: &[crate::analyze::EventReport]) -> Value {
    Value::Arr(
        evs.iter()
            .map(|e| {
                let mut m = BTreeMap::new();
                m.insert("event".into(), Value::Str(e.event.clone()));
                m.insert(
                    "status".into(),
                    Value::Str(
                        match e.status {
                            crate::analyze::EventStatus::Enabled => "enabled",
                            crate::analyze::EventStatus::Disabled => "disabled",
                            crate::analyze::EventStatus::DependsOnPayload => "depends_on_payload",
                            crate::analyze::EventStatus::Preempted => "preempted",
                            crate::analyze::EventStatus::PreemptedMaybe => "preempted_maybe",
                        }
                        .into(),
                    ),
                );
                if !e.payload_fields.is_empty() {
                    m.insert(
                        "payload_fields".into(),
                        Value::Arr(e.payload_fields.iter().cloned().map(Value::Str).collect()),
                    );
                }
                Value::Obj(m)
            })
            .collect(),
    )
}
