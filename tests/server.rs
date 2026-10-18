use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use siga::{server::app, spec::Spec};
use std::path::Path;
use tower::ServiceExt;

const KEY: (&str, &str) = ("X-API-Key", "my-valid-key");

/// Sends one request to the petstore emulator and returns the status and JSON body.
async fn call(
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (StatusCode, Value) {
    let spec = Spec::load(Path::new("tests/data/petstore.yaml")).unwrap();
    let mut req = Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let res = app(spec)
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn generates_values_within_the_schema() {
    for _ in 0..20 {
        let (status, pets) = call("GET", "/v1/pets", &[KEY], "").await;
        assert_eq!(status, StatusCode::OK);
        let pets = pets.as_array().unwrap();
        assert_eq!(pets.len(), 3);
        for p in pets {
            assert!((1..=100).contains(&p["id"].as_i64().unwrap()), "{p}");
            assert!(
                ["available", "pending", "sold"].contains(&p["status"].as_str().unwrap()),
                "{p}"
            );
            assert!((0.5..=80.0).contains(&p["weight"].as_f64().unwrap()), "{p}");
            assert!(p["name"].is_string() && p["vaccinated"].is_boolean(), "{p}");
            let tags = p["tags"].as_array().unwrap();
            assert_eq!(tags.len(), 4, "{p}");
            assert!(
                tags.iter().enumerate().all(|(i, t)| !tags[..i].contains(t)),
                "{p}"
            );
            assert_eq!(p["owner"]["contacts"].as_array().unwrap().len(), 3, "{p}");
        }
    }
}

#[tokio::test]
async fn api_key() {
    assert_eq!(
        call("GET", "/v1/pets", &[], "").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call("GET", "/v1/pets", &[("X-API-Key", "my-invalid-key")], "")
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call("GET", "/v1/health", &[], "").await,
        (StatusCode::OK, json!({ "status": "up" }))
    );
}

#[tokio::test]
async fn bearer_jwt() {
    for (token, want) in [
        ("Bearer my-valid-token", StatusCode::OK),
        ("Bearer my-invalid-token", StatusCode::UNAUTHORIZED),
        ("Bearer aaa.bbb.ccc", StatusCode::UNAUTHORIZED),
        ("Bearer my-valid-key", StatusCode::UNAUTHORIZED),
        ("my-valid-token", StatusCode::UNAUTHORIZED),
    ] {
        assert_eq!(
            call("GET", "/v1/pets/7", &[("Authorization", token)], "")
                .await
                .0,
            want,
            "{token}"
        );
    }
    assert_eq!(
        call("GET", "/v1/pets/7", &[], "").await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn oauth_scopes() {
    let body = r#"{"name": "rex", "status": "sold"}"#;
    let (status, pet) = call("POST", "/v1/pets?force-auth=read,write", &[], body).await;
    assert_eq!(status, StatusCode::OK);
    assert!(pet["id"].is_i64());
    for q in ["", "?force-auth=no", "?force-auth=read"] {
        assert_eq!(
            call("POST", &format!("/v1/pets{q}"), &[], body).await.0,
            StatusCode::UNAUTHORIZED,
            "{q}"
        );
    }
    assert_eq!(
        call("DELETE", "/v1/pets/1?force-auth=write", &[], "")
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call("DELETE", "/v1/pets/1?force-auth=write,admin", &[], "").await,
        (StatusCode::OK, Value::Null)
    );
}

#[tokio::test]
async fn rejects_bad_bodies() {
    let cases = [
        ("", "not valid JSON"),
        ("{not json", "not valid JSON"),
        (r#"{"status": "sold"}"#, "body.name: required"),
        (r#"{"name": "", "status": "sold"}"#, "body.name: expected"),
        (
            r#"{"name": "rex", "status": "lost"}"#,
            "body.status: expected",
        ),
        (
            r#"{"name": "rex", "status": "sold", "tags": [1]}"#,
            "body.tags[0]: expected a value of type string",
        ),
        ("[]", "body: expected a value of type object"),
    ];
    for (body, want) in cases {
        let (status, err) = call("PUT", "/v1/pets/1?force-auth=write", &[], body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(
            err["error"].as_str().unwrap().contains(want),
            "{body}: {err}"
        );
    }
}

#[tokio::test]
async fn force_error() {
    assert_eq!(
        call("GET", "/v1/pets?force-error=418", &[KEY], "").await,
        (StatusCode::IM_A_TEAPOT, Value::Null)
    );
    let (status, body) = call("POST", "/v1/pets?force-auth=write&force-error=503", &[], "").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        (1..=60).contains(&body["retryAfter"].as_i64().unwrap()),
        "{body}"
    );
    assert_eq!(
        call("GET", "/v1/pets?force-error=abc", &[KEY], "").await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn routing() {
    let (status, body) = call("GET", "/v1/pets/count", &[KEY], "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        (0..=10).contains(&body["count"].as_i64().unwrap()),
        "{body}"
    );
    assert_eq!(
        call("GET", "/v1/nope", &[KEY], "").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call("GET", "/pets", &[KEY], "").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call("PATCH", "/v1/pets", &[KEY], "").await.0,
        StatusCode::METHOD_NOT_ALLOWED
    );
}
