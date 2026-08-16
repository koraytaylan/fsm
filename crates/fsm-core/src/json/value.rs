//! JSON value model. Numbers keep their raw token text; objects use `BTreeMap`.

use std::collections::BTreeMap;

/// A JSON value. `Num` holds the raw number token verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<Value>),
    Obj(BTreeMap<String, Value>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_obj(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Obj(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&[Value]> {
        match self {
            Value::Arr(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_num(&self) -> Option<&str> {
        match self {
            Value::Num(s) => Some(s),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_obj()?.get(key)
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn is_bool(&self) -> bool {
        matches!(self, Value::Bool(_))
    }

    pub fn is_num(&self) -> bool {
        matches!(self, Value::Num(_))
    }

    pub fn is_str(&self) -> bool {
        matches!(self, Value::Str(_))
    }

    pub fn is_arr(&self) -> bool {
        matches!(self, Value::Arr(_))
    }

    pub fn is_obj(&self) -> bool {
        matches!(self, Value::Obj(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_on_nested_obj() {
        let mut inner = BTreeMap::new();
        inner.insert("b".into(), Value::Bool(true));
        let mut outer = BTreeMap::new();
        outer.insert("a".into(), Value::Obj(inner));
        let v = Value::Obj(outer);
        assert_eq!(
            v.get("a").and_then(|a| a.get("b")).and_then(Value::as_bool),
            Some(true)
        );
        assert!(v.get("missing").is_none());
    }

    #[test]
    fn accessors_none_on_mismatch() {
        let n = Value::Null;
        assert!(n.as_str().is_none());
        assert!(n.as_obj().is_none());
        assert!(n.as_arr().is_none());
        assert!(n.as_bool().is_none());
        assert!(n.as_num().is_none());
        assert!(n.get("x").is_none());
        assert!(n.is_null());
        assert!(!n.is_bool());
        assert!(!n.is_num());
        assert!(!n.is_str());
        assert!(!n.is_arr());
        assert!(!n.is_obj());
        assert!(Value::Bool(true).as_str().is_none());
        assert!(Value::Num("1".into()).as_bool().is_none());
        assert!(Value::Str("x".into()).as_arr().is_none());
        assert!(Value::Arr(vec![]).as_obj().is_none());
        assert!(Value::Obj(BTreeMap::new()).as_num().is_none());
    }

    #[test]
    fn eq_structurally_equal() {
        let mut a = BTreeMap::new();
        a.insert("k".into(), Value::Num("1".into()));
        let mut b = BTreeMap::new();
        b.insert("k".into(), Value::Num("1".into()));
        assert_eq!(Value::Obj(a), Value::Obj(b));
        assert_eq!(Value::Arr(vec![Value::Null]), Value::Arr(vec![Value::Null]));
        assert_ne!(Value::Num("1".into()), Value::Num("1.0".into()));
    }
}
