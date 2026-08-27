//! The honest security boundary: loopback, `Origin`, and a bearer token.
//!
//! There is no TLS here and there will not be one. Exposing this beyond
//! loopback is an operator's decision made behind a proxy that terminates
//! TLS, and `7302` says so rather than implying a property the code does not
//! have.
//!
//! Plan 0015 tasks 7101, 7102 and 7103 fill this in.

/// Whether an `Origin` header is one this server answers.
pub fn origin_allowed(_origin: Option<&str>, _allowed: &[String]) -> bool {
    unimplemented!("plan 0015 task 7101")
}

/// Compare a presented token with the configured one in constant time.
pub fn token_matches(_presented: &str, _configured: &str) -> bool {
    unimplemented!("plan 0015 task 7102")
}
