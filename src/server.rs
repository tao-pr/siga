//! The HTTP server that emulates the spec.
use crate::{
    auth, schema,
    spec::{Spec, json},
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use openapiv3::{Operation, PathItem};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};

/// Builds the router that answers every request from the spec.
pub fn app(spec: Spec) -> Router {
    Router::new().fallback(handle).with_state(Arc::new(spec))
}

/// Serves the spec on `127.0.0.1` at the spec's port until the process stops.
pub async fn serve(spec: Spec) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", spec.port)).await?;
    println!(
        "Siga emulates {:?} on http://127.0.0.1:{}{}",
        spec.api.info.title, spec.port, spec.base
    );
    for (path, method, _) in spec.api.operations() {
        println!("  {:<7} {}{path}", method.to_uppercase(), spec.base);
    }
    axum::serve(listener, app(spec)).await?;
    Ok(())
}

async fn handle(
    State(spec): State<Arc<Spec>>,
    method: Method,
    uri: Uri,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(item) = route(&spec, uri.path()) else {
        return fail(StatusCode::NOT_FOUND, "the path is not in the spec");
    };
    let Some((_, op)) = item
        .iter()
        .find(|(m, _)| m.eq_ignore_ascii_case(method.as_str()))
    else {
        return fail(
            StatusCode::METHOD_NOT_ALLOWED,
            "the method is not in the spec for this path",
        );
    };
    if !auth::allowed(&spec, op, &headers, &query) {
        return fail(StatusCode::UNAUTHORIZED, "authentication failed");
    }
    if let Some(code) = query.get("force-error") {
        return match code.parse().ok().and_then(|c| StatusCode::from_u16(c).ok()) {
            Some(code) => reply(&spec, op, code),
            None => fail(
                StatusCode::BAD_REQUEST,
                "force-error must be an HTTP status code",
            ),
        };
    }
    if let Some((s, required)) = spec.req(op)
        && (required || !body.is_empty())
    {
        let v = match serde_json::from_slice::<Value>(&body) {
            Ok(v) => v,
            Err(e) => {
                return fail(
                    StatusCode::BAD_REQUEST,
                    &format!("the body is not valid JSON: {e}"),
                );
            }
        };
        if let Err(e) = schema::check(&spec, s, &v, "body") {
            return fail(StatusCode::BAD_REQUEST, &e);
        }
    }
    reply(&spec, op, StatusCode::OK)
}

/// Finds the path item for a request path, preferring literal segments over templates.
fn route<'a>(spec: &'a Spec, path: &str) -> Option<&'a PathItem> {
    let path = path.strip_prefix(&spec.base)?;
    let fits = |tpl: &str| {
        let (t, p) = (tpl.split('/'), path.split('/'));
        t.clone().count() == p.clone().count()
            && t.zip(p)
                .all(|(t, p)| t == p || t.starts_with('{') && !p.is_empty())
    };
    let (_, item) = spec
        .api
        .paths
        .paths
        .iter()
        .filter(|(t, _)| fits(t))
        .min_by_key(|(t, _)| t.matches('{').count())?;
    item.as_item()
}

/// Answers with `code` and a random body from the matching response schema, if any.
fn reply(spec: &Spec, op: &Operation, code: StatusCode) -> Response {
    match spec.res(op, code.as_u16()).and_then(|r| json(&r.content)) {
        Some(s) => (code, Json(schema::fake(spec, s, 0))).into_response(),
        None => code.into_response(),
    }
}

fn fail(code: StatusCode, msg: &str) -> Response {
    (code, Json(serde_json::json!({ "error": msg }))).into_response()
}
