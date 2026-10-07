//! Owned original-domain retirement; never claim settlement evidence.

use super::{NativeHelperProgress, NativeRequest};
use fsm_core::json::Value;
use fsm_core::record::execution::NativeDomain;
use std::collections::BTreeMap;
use std::time::Duration;

/// Cleanup of a delivered unclaimed domain, with its original route retained.
pub struct NativePreparedCleanup {
    request: NativeRequest,
    original: Value,
    closed: bool,
    error: Option<String>,
}

impl NativePreparedCleanup {
    /// Request retirement of this complete original domain without discovery.
    /// Root refuses any binding or submission material; no claim receipt is issued.
    pub fn start(domain: &NativeDomain, timeout: Duration) -> Result<Self, String> {
        let original = domain.to_value();
        let namespace = original
            .get("namespace")
            .and_then(Value::as_str)
            .ok_or("prepared cleanup namespace missing")?;
        let generation = original
            .get("generation")
            .and_then(Value::as_num)
            .ok_or("prepared cleanup generation missing")?
            .parse()
            .map_err(|_| "prepared cleanup generation invalid")?;
        let request = Value::Obj(BTreeMap::from([
            ("format".into(), Value::Str("fsm.native-request/1".into())),
            ("action".into(), Value::Str("discard-prepared".into())),
            ("payload".into(), original.clone()),
        ]));
        Ok(Self {
            request: NativeRequest::start(namespace, generation, &request, timeout)?,
            original,
            closed: false,
            error: None,
        })
    }

    /// Observe bounded cleanup; true requires matched success, helper reap and EOF.
    /// This never supplies execution closure proof or consumes journal ownership.
    pub fn poll(&mut self) -> Result<bool, String> {
        if self.closed {
            return Ok(true);
        }
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        let result = self.request.poll().and_then(|response| {
            response.map_or(Ok(false), |response| {
                validate(&response, &self.original).map(|()| true)
            })
        });
        match result {
            Ok(closed) => {
                self.closed = closed;
                Ok(closed)
            }
            Err(error) => {
                self.error = Some(error.clone());
                let _ = self.request.cancel();
                Err(error)
            }
        }
    }

    /// Cancel transport without asserting domain closure; matched success stays retained.
    pub fn cancel(&mut self) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        self.error.get_or_insert_with(|| {
            "prepared cleanup cancelled; original domain remains uncertain".into()
        });
        self.request.cancel()
    }

    /// Observe helper retirement without promoting it to domain closure.
    pub fn reap(&mut self) -> Result<bool, String> {
        self.request.reap()
    }

    /// Read identifier-free helper facts; these alone never release a reservation.
    pub fn progress(&self) -> NativeHelperProgress {
        self.request.progress()
    }
}

fn validate(response: &Value, original: &Value) -> Result<(), String> {
    if response.as_obj().is_some_and(|fields| fields.len() == 3)
        && response.get("format").and_then(Value::as_str) == Some("fsm.native-response/1")
        && response.get("ok") == Some(&Value::Bool(false))
        && let Some(reason) = response.get("result").and_then(Value::as_str)
    {
        return Err(format!(
            "prepared cleanup refused: {}",
            reason.chars().take(1024).collect::<String>()
        ));
    }
    if response.as_obj().is_some_and(|fields| fields.len() == 3)
        && response.get("format").and_then(Value::as_str) == Some("fsm.native-response/1")
        && response.get("ok") == Some(&Value::Bool(true))
        && response.get("result") == Some(original)
    {
        Ok(())
    } else {
        Err("prepared cleanup response does not match original domain".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusal_changed_identity_and_extra_fields_cannot_confirm_cleanup() {
        let original = Value::Obj(BTreeMap::from([
            ("allocation".into(), Value::Num("7".into())),
            ("authority".into(), Value::Num("43".into())),
        ]));
        let mut response = BTreeMap::from([
            ("format".into(), Value::Str("fsm.native-response/1".into())),
            ("ok".into(), Value::Bool(true)),
            ("result".into(), original.clone()),
        ]);
        assert!(validate(&Value::Obj(response.clone()), &original).is_ok());
        response.insert("ok".into(), Value::Bool(false));
        assert!(validate(&Value::Obj(response.clone()), &original).is_err());
        response.insert("result".into(), Value::Str("authority busy".into()));
        assert_eq!(
            validate(&Value::Obj(response.clone()), &original).unwrap_err(),
            "prepared cleanup refused: authority busy"
        );
        response.insert("ok".into(), Value::Bool(true));
        response.insert("result".into(), Value::Null);
        assert!(validate(&Value::Obj(response.clone()), &original).is_err());
        let mut changed = original.as_obj().unwrap().clone();
        changed.insert("authority".into(), Value::Num("44".into()));
        response.insert("result".into(), Value::Obj(changed));
        assert!(validate(&Value::Obj(response.clone()), &original).is_err());
        response.insert("result".into(), original.clone());
        response.insert("receipt".into(), Value::Null);
        assert!(validate(&Value::Obj(response), &original).is_err());
    }
}
