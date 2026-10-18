use siga::spec::Spec;
use std::path::Path;

const HEAD: &str =
    "openapi: 3.0.3\ninfo: {title: t, version: '1'}\nservers: [{url: 'http://localhost:8080'}]\n";
const OK: &str = "{'200': {description: ok, content: {application/json: {schema: {type: object, properties: {a: {type: string}}}}}}}";

/// Validates a spec with `HEAD` and the given `paths` YAML, and returns the error message.
fn error(rest: &str) -> String {
    let api = serde_yaml::from_str(&format!("{HEAD}{rest}")).unwrap();
    Spec::new(api)
        .expect_err("the spec must be rejected")
        .to_string()
}

#[test]
fn loads_yaml_and_json() {
    let spec = Spec::load(Path::new("tests/data/petstore.yaml")).unwrap();
    assert_eq!((spec.port, spec.base.as_str()), (8080, "/v1"));
    let spec = Spec::load(Path::new("tests/data/ping.json")).unwrap();
    assert_eq!((spec.port, spec.base.as_str()), (9090, ""));
}

#[test]
fn rejects_bad_files() {
    let err = Spec::load(Path::new("Cargo.toml")).unwrap_err().to_string();
    assert!(err.contains(".yaml, .yml or .json"), "{err}");
    assert!(Spec::load(Path::new("tests/data/missing.yaml")).is_err());
}

#[test]
fn requires_port() {
    let api = serde_yaml::from_str(&format!("{}paths: {{}}", HEAD.replace(":8080", ""))).unwrap();
    let err = Spec::new(api).unwrap_err().to_string();
    assert!(err.contains("port"), "{err}");
}

#[test]
fn rejects_unsupported_features() {
    let cases = [
        ("paths: {/a: {}}", "no operation is defined"),
        (
            "paths: {/a: {get: {responses: {'404': {description: no}}}}}",
            "\"200\" response is required",
        ),
        (
            "paths: {/a: {get: {responses: {'200': {description: ok}}}}}",
            "needs a JSON schema",
        ),
        (
            &format!("paths: {{/a: {{post: {{responses: {OK}}}}}}}"),
            "request body",
        ),
        (
            "paths: {/a: {get: {responses: {'200': {description: ok, content: {application/json: {schema: {type: object}}}}}}}}",
            "free-form",
        ),
        (
            "paths: {/a: {get: {responses: {'200': {description: ok, content: {application/json: {schema: {$ref: '#/components/schemas/Nope'}}}}}}}}",
            "unresolved $ref",
        ),
        (
            "paths: {/a: {get: {responses: {'200': {description: ok, content: {application/json: {schema: {not: {type: string}}}}}}}}}",
            "`not` is not supported",
        ),
        (
            &format!(
                "paths: {{/a: {{get: {{responses: {OK}, callbacks: {{cb: {{'{{$request.body#/url}}': {{post: {{responses: {OK}}}}}}}}}}}}}}}"
            ),
            "callbacks are not supported",
        ),
        (
            &format!("paths: {{/a: {{get: {{security: [{{nope: []}}], responses: {OK}}}}}}}"),
            "\"nope\" is not defined",
        ),
    ];
    for (rest, want) in cases {
        let err = error(rest);
        assert!(err.contains(want), "expected {want:?} in: {err}");
    }
}
