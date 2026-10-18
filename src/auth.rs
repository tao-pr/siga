//! Emulated authentication for the security schemes in the spec.
use crate::spec::Spec;
use axum::http::HeaderMap;
use openapiv3::{APIKeyLocation, Operation, SecurityScheme};
use std::collections::HashMap;

/// The only API key that passes authentication.
pub const VALID_KEY: &str = "my-valid-key";

/// The only bearer token, JWT or not, that passes authentication.
pub const VALID_TOKEN: &str = "my-valid-token";

/// Returns whether the request satisfies one of the security requirements of `op`.
pub fn allowed(
    spec: &Spec,
    op: &Operation,
    headers: &HeaderMap,
    query: &HashMap<String, String>,
) -> bool {
    let Some(reqs) = op.security.as_ref().or(spec.api.security.as_ref()) else {
        return true;
    };
    let header = |n: &str| headers.get(n).and_then(|v| v.to_str().ok());
    reqs.is_empty()
        || reqs.iter().any(|req| {
            req.iter().all(|(name, scopes)| match spec.scheme(name) {
                Some(SecurityScheme::APIKey { location, name, .. }) => {
                    let key = match location {
                        APIKeyLocation::Header => header(name),
                        APIKeyLocation::Query => query.get(name).map(String::as_str),
                        APIKeyLocation::Cookie => header("cookie").and_then(|c| {
                            c.split(';').find_map(|kv| {
                                kv.trim().strip_prefix(name.as_str())?.strip_prefix('=')
                            })
                        }),
                    };
                    key == Some(VALID_KEY)
                }
                Some(SecurityScheme::HTTP { .. }) => {
                    header("authorization").and_then(|a| a.strip_prefix("Bearer "))
                        == Some(VALID_TOKEN)
                }
                Some(SecurityScheme::OAuth2 { .. }) => query.get("force-auth").is_some_and(|g| {
                    g != "no" && scopes.iter().all(|s| g.split(',').any(|x| x.trim() == s))
                }),
                _ => false,
            })
        })
}
