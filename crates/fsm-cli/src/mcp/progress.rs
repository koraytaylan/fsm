//! Progress on a call that takes long enough to say something about.
//!
//! A skeleton: plan 0012 task 6002 fills it.

use fsm_core::json::Value;

/// The parameters of a `notifications/progress`.
pub fn progress_params(_token: &Value, _progress: u64, _total: Option<u64>) -> Value {
    unimplemented!("plan 0012 task 6002")
}

/// The progress token a request carried, if it carried one.
pub fn token_of(_params: &Value) -> Option<Value> {
    unimplemented!("plan 0012 task 6002")
}
