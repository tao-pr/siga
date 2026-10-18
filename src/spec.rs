//! Loading and validating OpenAPI specs.
use anyhow::{Context, Result, bail};
use axum::http::Uri;
use indexmap::IndexMap;
use openapiv3::*;
use std::{borrow::Borrow, collections::HashSet, path::Path};

/// A validated OpenAPI spec that Siga can emulate.
#[derive(Debug)]
pub struct Spec {
    pub api: OpenAPI,
    pub port: u16,
    /// Path prefix from the server URL, e.g. `/v1`, or empty.
    pub base: String,
}

impl Spec {
    /// Reads a `.yaml`, `.yml` or `.json` spec file and validates it.
    pub fn load(path: &Path) -> Result<Spec> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let api = match path.extension().and_then(|e| e.to_str()) {
            Some("json") => serde_json::from_str(&text)?,
            Some("yaml" | "yml") => serde_yaml::from_str(&text)?,
            _ => bail!(
                "{}: spec file must end in .yaml, .yml or .json",
                path.display()
            ),
        };
        Spec::new(api)
    }

    /// Validates `api` and reads the port and base path from its first server URL.
    pub fn new(api: OpenAPI) -> Result<Spec> {
        let url = api.servers.first().map_or("", |s| s.url.as_str());
        let (port, base) = url
            .parse::<Uri>()
            .ok()
            .and_then(|u| Some((u.port_u16()?, u.path().trim_end_matches('/').to_string())))
            .with_context(|| {
                format!("servers[0].url must be an absolute URL with a port, found {url:?}")
            })?;
        let spec = Spec { api, port, base };
        let errs = spec.lint();
        if !errs.is_empty() {
            bail!(
                "the spec is not supported by Siga:\n  - {}",
                errs.join("\n  - ")
            );
        }
        Ok(spec)
    }

    /// Resolves a schema that may be a `$ref` to `#/components/schemas`.
    pub fn schema<'a, S: Borrow<Schema>>(&'a self, r: &'a ReferenceOr<S>) -> Option<&'a Schema> {
        deref(r, self.comps().map(|c| &c.schemas), "schemas")
    }

    /// The JSON request body schema of `op`, and whether the body is required.
    pub fn req<'a>(&'a self, op: &'a Operation) -> Option<(&'a ReferenceOr<Schema>, bool)> {
        let body = deref(
            op.request_body.as_ref()?,
            self.comps().map(|c| &c.request_bodies),
            "requestBodies",
        )?;
        Some((json(&body.content)?, body.required))
    }

    /// The response of `op` for `code`, falling back to its `NXX` range and then `default`.
    pub fn res<'a>(&'a self, op: &'a Operation, code: u16) -> Option<&'a Response> {
        let all = &op.responses.responses;
        let r = all
            .get(&StatusCode::Code(code))
            .or(all.get(&StatusCode::Range(code / 100)));
        deref(
            r.or(op.responses.default.as_ref())?,
            self.comps().map(|c| &c.responses),
            "responses",
        )
    }

    /// Looks up a security scheme by name.
    pub fn scheme(&self, name: &str) -> Option<&SecurityScheme> {
        self.comps()?.security_schemes.get(name)?.as_item()
    }

    fn comps(&self) -> Option<&Components> {
        self.api.components.as_ref()
    }

    /// Lists every reason why Siga cannot emulate the spec.
    fn lint(&self) -> Vec<String> {
        let (mut errs, mut seen) = (Vec::new(), HashSet::new());
        for (path, item) in &self.api.paths.paths {
            let Some(item) = item.as_item() else {
                errs.push(format!("{path}: path item $ref is not supported"));
                continue;
            };
            if item.iter().next().is_none() {
                errs.push(format!("{path}: no operation is defined"));
            }
            for (method, op) in item.iter() {
                let at = format!("{} {path}", method.to_uppercase());
                if !op.callbacks.is_empty() {
                    errs.push(format!("{at}: callbacks are not supported"));
                }
                match self.req(op) {
                    Some((s, _)) => {
                        self.lint_schema(s, &format!("{at} request"), &mut seen, &mut errs)
                    }
                    None if matches!(method, "post" | "put" | "patch") => errs.push(format!(
                        "{at}: a request body with a JSON schema is required"
                    )),
                    None => {}
                }
                let ok = op.responses.responses.get(&StatusCode::Code(200));
                if ok.is_none() {
                    errs.push(format!("{at}: a \"200\" response is required"));
                } else if matches!(method, "get" | "post" | "put" | "patch")
                    && self.res(op, 200).and_then(|r| json(&r.content)).is_none()
                {
                    errs.push(format!("{at}: the \"200\" response needs a JSON schema"));
                }
                for r in op.responses.responses.values().chain(&op.responses.default) {
                    match deref(r, self.comps().map(|c| &c.responses), "responses") {
                        Some(r) => json(&r.content).into_iter().for_each(|s| {
                            self.lint_schema(s, &format!("{at} response"), &mut seen, &mut errs)
                        }),
                        None => errs.push(format!("{at}: unresolved response $ref")),
                    }
                }
            }
        }
        let ops = self
            .api
            .operations()
            .filter_map(|(_, _, op)| op.security.as_ref());
        for name in self
            .api
            .security
            .iter()
            .chain(ops)
            .flatten()
            .flat_map(|r| r.keys())
        {
            match self.scheme(name) {
                None => errs.push(format!("security scheme {name:?} is not defined")),
                Some(SecurityScheme::HTTP { scheme, .. }) if !scheme.eq_ignore_ascii_case("bearer") => {
                    errs.push(format!("security scheme {name:?}: http scheme {scheme:?} is not supported, use bearer"))
                }
                Some(SecurityScheme::OpenIDConnect { .. }) => {
                    errs.push(format!("security scheme {name:?}: openIdConnect is not supported"))
                }
                _ => {}
            }
        }
        errs.sort();
        errs.dedup();
        errs
    }

    /// Checks that a schema resolves, has a type, and is not free-form.
    fn lint_schema<S: Borrow<Schema>>(
        &self,
        r: &ReferenceOr<S>,
        at: &str,
        seen: &mut HashSet<String>,
        errs: &mut Vec<String>,
    ) {
        let at = match r {
            ReferenceOr::Reference { reference } if !seen.insert(reference.clone()) => return,
            ReferenceOr::Reference { reference } => reference.clone(),
            ReferenceOr::Item(_) => at.to_string(),
        };
        let Some(s) = self.schema(r) else {
            return errs.push(format!("{at}: unresolved $ref"));
        };
        match &s.schema_kind {
            SchemaKind::Type(Type::Object(o)) => {
                let typed = matches!(
                    &o.additional_properties,
                    Some(AdditionalProperties::Schema(_))
                );
                if let Some(AdditionalProperties::Schema(a)) = &o.additional_properties {
                    self.lint_schema(a.as_ref(), &format!("{at}.*"), seen, errs);
                }
                if o.properties.is_empty() && !typed {
                    errs.push(format!("{at}: free-form objects are not supported"));
                }
                for (k, p) in &o.properties {
                    self.lint_schema(p, &format!("{at}.{k}"), seen, errs);
                }
            }
            SchemaKind::Type(Type::Array(a)) => match &a.items {
                Some(i) => self.lint_schema(i, &format!("{at}[]"), seen, errs),
                None => errs.push(format!("{at}: an array must define items")),
            },
            SchemaKind::OneOf { one_of: v }
            | SchemaKind::AnyOf { any_of: v }
            | SchemaKind::AllOf { all_of: v } => {
                for (i, x) in v.iter().enumerate() {
                    self.lint_schema(x, &format!("{at}/{i}"), seen, errs);
                }
            }
            SchemaKind::Not { .. } => errs.push(format!("{at}: `not` is not supported")),
            SchemaKind::Any(_) => errs.push(format!(
                "{at}: the schema must declare a type (free-form schemas are not supported)"
            )),
            SchemaKind::Type(_) => {}
        }
    }
}

/// The schema of the first JSON media type in `content`.
pub fn json(content: &IndexMap<String, MediaType>) -> Option<&ReferenceOr<Schema>> {
    content
        .iter()
        .find(|(k, _)| k.contains("json"))?
        .1
        .schema
        .as_ref()
}

/// Resolves `r` inline or from `#/components/{kind}/{name}` in `pool`.
fn deref<'a, T, S: Borrow<T>>(
    r: &'a ReferenceOr<S>,
    pool: Option<&'a IndexMap<String, ReferenceOr<T>>>,
    kind: &str,
) -> Option<&'a T> {
    match r {
        ReferenceOr::Item(x) => Some(x.borrow()),
        ReferenceOr::Reference { reference } => {
            let name = reference
                .strip_prefix("#/components/")?
                .strip_prefix(kind)?
                .strip_prefix('/')?;
            pool?.get(name)?.as_item()
        }
    }
}
