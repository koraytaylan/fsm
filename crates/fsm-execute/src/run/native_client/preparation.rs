//! Owned domain preparation precedes a durable claim and grants no handler entry.

use super::{NativeHelperProgress, NativeRequest};
use fsm_core::json::Value;
use fsm_core::record::execution::NativeDomain;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Domain preparation state, never handler closure or durable ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativePreparationPhase {
    /// A prepared domain is requested; no handler is authorized.
    Preparing,
    /// The route-matched domain was delivered once after actual helper retirement.
    Prepared,
    /// Preparation failed or was cancelled; no domain has been delivered.
    Uncertain,
}

/// Identifier-free preparation progress, separate from domain/claim permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativePreparationProgress {
    /// Whether a prepared domain has been delivered.
    pub phase: NativePreparationPhase,
    /// Actual transport helper retirement and stream EOF observations.
    pub helper: NativeHelperProgress,
}

/// Owned allocator request; callers must durably claim before binding or launch.
pub struct NativePreparation {
    request: NativeRequest,
    namespace: String,
    generation: u64,
    deadline: Instant,
    delivered: bool,
    error: Option<String>,
    owned: bool,
}

impl NativePreparation {
    /// Discover one protected physical-store route and request an empty domain.
    /// This grants no handler entry or durable ownership.
    pub fn for_store(store: &std::path::Path, timeout: Duration) -> Result<Self, String> {
        let (namespace, generation) = super::discovery::discover(store)?;
        Self::start(&namespace, generation, timeout)
    }

    /// Discover the original physical-store route and request owned preparation.
    pub fn for_store_owned(store: &std::path::Path, timeout: Duration) -> Result<Self, String> {
        let (namespace, generation) = super::discovery::discover(store)?;
        Self::start_owned(&namespace, generation, timeout)
    }

    /// Request an empty prepared domain from the fixed provisioned authority.
    pub fn start(namespace: &str, generation: u64, timeout: Duration) -> Result<Self, String> {
        Self::start_action(namespace, generation, timeout, "prepare")
    }

    /// Request a domain whose original operator lease must be collected with it.
    pub fn start_owned(
        namespace: &str,
        generation: u64,
        timeout: Duration,
    ) -> Result<Self, String> {
        Self::start_action(namespace, generation, timeout, "prepare-owned")
    }

    fn start_action(
        namespace: &str,
        generation: u64,
        timeout: Duration,
        action: &str,
    ) -> Result<Self, String> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or("native preparation deadline exceeds clock range")?;
        let request = Value::Obj(BTreeMap::from([
            ("format".into(), Value::Str("fsm.native-request/1".into())),
            ("action".into(), Value::Str(action.into())),
            ("payload".into(), Value::Null),
        ]));
        Ok(Self {
            request: NativeRequest::start(namespace, generation, &request, timeout)?,
            namespace: namespace.into(),
            generation,
            deadline,
            delivered: false,
            error: None,
            owned: action == "prepare-owned",
        })
    }

    /// Collect one original-route domain only after helper success, reap and EOF.
    pub fn poll(&mut self) -> Result<Option<NativeDomain>, String> {
        if self.owned {
            return Err("owned preparation requires poll_owned".into());
        }
        self.collect()
    }

    /// Collect metadata only together with its exclusively held original lease.
    pub fn poll_owned(&mut self) -> Result<Option<super::NativePreparedOwner>, String> {
        if !self.owned {
            return Err("legacy preparation has no owner lease".into());
        }
        let result = self
            .collect()?
            .map(super::NativePreparedOwner::acquire)
            .transpose()
            .and_then(|owner| {
                // Lease authentication must not extend the original request
                // deadline or expose a late domain for claim publication.
                if owner.is_some() && Instant::now() >= self.deadline {
                    Err("native preparation deadline; allocation remains uncertain".into())
                } else {
                    Ok(owner)
                }
            });
        if let Err(error) = &result {
            self.error = Some(error.clone());
            self.delivered = false;
        }
        result
    }

    fn collect(&mut self) -> Result<Option<NativeDomain>, String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if self.delivered {
            return Err("native prepared domain already collected".into());
        }
        let result = if Instant::now() >= self.deadline {
            Err("native preparation deadline; allocation remains uncertain".into())
        } else {
            self.request.poll().and_then(|response| {
                let domain = response
                    .map(|value| decode(&value, &self.namespace, self.generation))
                    .transpose()?;
                if Instant::now() >= self.deadline {
                    return Err("native preparation deadline; allocation remains uncertain".into());
                }
                Ok(domain)
            })
        };
        match &result {
            Ok(Some(_)) => self.delivered = true,
            Err(error) => {
                self.error = Some(error.clone());
                let _ = self.request.cancel();
            }
            Ok(None) => {}
        }
        result
    }

    /// Cancel helper work without asserting domain absence or cleanup authority.
    pub fn cancel(&mut self) -> Result<(), String> {
        self.error.get_or_insert_with(|| {
            "native preparation cancelled; allocation may remain uncertain".into()
        });
        self.request.cancel()
    }

    /// Read bounded progress without polling, claiming or releasing a domain.
    pub fn progress(&self) -> NativePreparationProgress {
        NativePreparationProgress {
            phase: if self.delivered {
                NativePreparationPhase::Prepared
            } else if self.error.is_some() {
                NativePreparationPhase::Uncertain
            } else {
                NativePreparationPhase::Preparing
            },
            helper: self.request.progress(),
        }
    }

    /// Observe actual helper retirement; this does not prove native domain closure.
    pub fn reap(&mut self) -> Result<bool, String> {
        self.request.reap()
    }
}

fn decode(response: &Value, namespace: &str, generation: u64) -> Result<NativeDomain, String> {
    if let Some(reason) = super::refusal(response, "native preparation refused: ") {
        return Err(reason);
    }
    let fields = response
        .as_obj()
        .ok_or("native preparation response is not an object")?;
    if fields.len() != 3
        || response.get("format").and_then(Value::as_str) != Some("fsm.native-response/1")
        || response.get("ok") != Some(&Value::Bool(true))
    {
        return Err("native preparation refused or response differs".into());
    }
    let value = response
        .get("result")
        .ok_or("native prepared domain missing")?;
    let domain = NativeDomain::from_value(value).map_err(|_| "native prepared domain invalid")?;
    if value.get("namespace").and_then(Value::as_str) != Some(namespace)
        || value.get("generation") != Some(&Value::Num(generation.to_string()))
    {
        return Err("native prepared domain route differs".into());
    }
    Ok(domain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fsm_core::json::{JsonLimits, parse};

    #[test]
    fn preparation_refusal_preserves_bounded_reason_without_delivering_domain() {
        let namespace = "0123456789abcdef0123456789abcdef";
        let prefix = "native preparation refused: ";
        let boundary = "a".repeat(1024 - prefix.len());
        for (reason, expected) in [
            ("authority\nbusy".into(), "authority busy".into()),
            (boundary.clone(), boundary.clone()),
            (boundary.clone() + "b", boundary.clone()),
            (boundary.clone() + "é", boundary),
        ] {
            let mut fields = BTreeMap::from([
                ("format".into(), Value::Str("fsm.native-response/1".into())),
                ("ok".into(), Value::Bool(false)),
                ("result".into(), Value::Str(reason)),
            ]);
            assert_eq!(
                decode(&Value::Obj(fields.clone()), namespace, 9).unwrap_err(),
                prefix.to_owned() + &expected
            );
            fields.insert("extra".into(), Value::Null);
            assert_eq!(
                decode(&Value::Obj(fields), namespace, 9).unwrap_err(),
                "native preparation refused or response differs"
            );
        }
    }

    #[test]
    fn prepared_metadata_requires_successful_original_route() {
        let namespace = "0123456789abcdef0123456789abcdef";
        let domain = parse(br#"{"backend":"linux-systemd/1","namespace":"0123456789abcdef0123456789abcdef","allocation":7,"boot":"01234567-89ab-cdef-0123-456789abcdef","cgroup":{"device":0,"inode":42},"authority":{"device":8,"inode":43},"generation":9}"#, &JsonLimits::DEFAULT).unwrap();
        let mut fields = BTreeMap::from([
            ("format".into(), Value::Str("fsm.native-response/1".into())),
            ("ok".into(), Value::Bool(true)),
            ("result".into(), domain.clone()),
        ]);
        assert_eq!(
            decode(&Value::Obj(fields.clone()), namespace, 9)
                .unwrap()
                .to_value(),
            domain
        );
        assert!(decode(&Value::Obj(fields.clone()), namespace, 10).is_err());
        assert!(
            decode(
                &Value::Obj(fields.clone()),
                "ffffffffffffffffffffffffffffffff",
                9
            )
            .is_err()
        );
        fields.insert("ok".into(), Value::Bool(false));
        assert!(decode(&Value::Obj(fields.clone()), namespace, 9).is_err());
        fields.insert("ok".into(), Value::Bool(true));
        fields.insert("result".into(), Value::Null);
        assert!(decode(&Value::Obj(fields), namespace, 9).is_err());
    }
}
