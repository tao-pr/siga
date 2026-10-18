//! Random value generation and validation against OpenAPI schemas.
use crate::spec::Spec;
use openapiv3::{AdditionalProperties, ReferenceOr, Schema, SchemaKind, Type};
use rand::{random_bool, random_range};
use serde_json::{Map, Value, json};
use std::borrow::Borrow;

const WORDS: &[&str] = &[
    "apple", "river", "stone", "cloud", "happy", "green", "table", "music", "light", "ocean",
    "tiger", "bread", "chair", "smile", "water", "dream", "house", "plant", "sunny", "quiet",
    "brave", "candle", "garden", "window", "forest", "silver", "orange", "rabbit", "pencil",
    "summer", "winter", "yellow", "basket", "mountain", "honey",
];

/// Schema nesting limit, which stops recursive schemas from looping forever.
const MAX_DEPTH: usize = 10;

/// Generates a random JSON value that follows the schema `r`.
pub fn fake<S: Borrow<Schema>>(spec: &Spec, r: &ReferenceOr<S>, depth: usize) -> Value {
    let Some(s) = spec.schema(r).filter(|_| depth < MAX_DEPTH) else {
        return Value::Null;
    };
    match &s.schema_kind {
        SchemaKind::Type(Type::String(t)) => pick(&t.enumeration).unwrap_or_else(|| {
            let words: Vec<_> = (0..random_range(1..=3)).map(|_| word()).collect();
            json!(
                words
                    .join(" ")
                    .chars()
                    .take(t.max_length.unwrap_or(usize::MAX))
                    .collect::<String>()
            )
        }),
        SchemaKind::Type(Type::Integer(t)) => pick(&t.enumeration).unwrap_or_else(|| {
            let lo = t.minimum.unwrap_or(
                t.maximum
                    .map_or(0, |m| if m < 0 { m.saturating_sub(1000) } else { 0 }),
            );
            let hi = t.maximum.unwrap_or(lo.saturating_add(1000)) - t.exclusive_maximum as i64;
            let lo = lo + t.exclusive_minimum as i64;
            json!(random_range(lo..=hi.max(lo)))
        }),
        SchemaKind::Type(Type::Number(t)) => pick(&t.enumeration).unwrap_or_else(|| {
            let lo = t
                .minimum
                .unwrap_or(t.maximum.map_or(0.0, |m| m.min(0.0) - 1000.0));
            let hi = t.maximum.unwrap_or(lo + 1000.0).max(lo);
            json!((random_range(lo..=hi) * 100.0).round() / 100.0)
        }),
        SchemaKind::Type(Type::Boolean(t)) => {
            pick(&t.enumeration).unwrap_or_else(|| json!(random_bool(0.5)))
        }
        SchemaKind::Type(Type::Array(a)) => {
            let n = a.min_items.unwrap_or(a.max_items.map_or(3, |m| m.min(3)));
            let mut out = Vec::new();
            for _ in 0..n * 20 {
                if out.len() == n {
                    break;
                }
                let v = a
                    .items
                    .as_ref()
                    .map_or(Value::Null, |i| fake(spec, i, depth + 1));
                if !a.unique_items || !out.contains(&v) {
                    out.push(v);
                }
            }
            Value::Array(out)
        }
        SchemaKind::Type(Type::Object(o)) => {
            let mut m: Map<_, _> = o
                .properties
                .iter()
                .map(|(k, p)| (k.clone(), fake(spec, p, depth + 1)))
                .collect();
            if let Some(AdditionalProperties::Schema(a)) = &o.additional_properties
                && o.properties.is_empty()
            {
                (0..3).for_each(|_| _ = m.insert(word().into(), fake(spec, a.as_ref(), depth + 1)));
            }
            Value::Object(m)
        }
        SchemaKind::OneOf { one_of: v } | SchemaKind::AnyOf { any_of: v } => v
            .get(random_range(0..v.len().max(1)))
            .map_or(Value::Null, |r| fake(spec, r, depth + 1)),
        SchemaKind::AllOf { all_of } => {
            let mut m = Map::new();
            for r in all_of {
                if let Value::Object(o) = fake(spec, r, depth + 1) {
                    m.extend(o);
                }
            }
            Value::Object(m)
        }
        SchemaKind::Not { .. } | SchemaKind::Any(_) => Value::Null,
    }
}

/// Checks that `v` follows the schema `r`, returning a message for the first mismatch at path `at`.
pub fn check<S: Borrow<Schema>>(
    spec: &Spec,
    r: &ReferenceOr<S>,
    v: &Value,
    at: &str,
) -> Result<(), String> {
    let Some(s) = spec.schema(r) else {
        return Ok(());
    };
    let need = |ok: bool, what: &str| {
        if ok {
            Ok(())
        } else {
            Err(format!("{at}: expected {what}"))
        }
    };
    if v.is_null() && s.schema_data.nullable {
        return Ok(());
    }
    match (&s.schema_kind, v) {
        (SchemaKind::Type(Type::String(t)), Value::String(x)) => need(
            allowed(&t.enumeration, x) && within(x.chars().count(), t.min_length, t.max_length),
            "a string within the enum and length limits",
        ),
        (SchemaKind::Type(Type::Integer(t)), Value::Number(n)) if n.is_i64() => {
            let x = n.as_i64().unwrap_or_default();
            let ok = t
                .minimum
                .is_none_or(|m| x > m || x == m && !t.exclusive_minimum)
                && t.maximum
                    .is_none_or(|m| x < m || x == m && !t.exclusive_maximum);
            need(
                ok && allowed(&t.enumeration, &x),
                "an integer within the enum and range limits",
            )
        }
        (SchemaKind::Type(Type::Number(t)), Value::Number(n)) => {
            let x = n.as_f64().unwrap_or_default();
            let ok = t
                .minimum
                .is_none_or(|m| x > m || x == m && !t.exclusive_minimum)
                && t.maximum
                    .is_none_or(|m| x < m || x == m && !t.exclusive_maximum);
            need(
                ok && allowed(&t.enumeration, &x),
                "a number within the enum and range limits",
            )
        }
        (SchemaKind::Type(Type::Boolean(t)), Value::Bool(b)) => {
            need(allowed(&t.enumeration, b), "a boolean in the enum")
        }
        (SchemaKind::Type(Type::Array(a)), Value::Array(xs)) => {
            need(
                within(xs.len(), a.min_items, a.max_items),
                "an array within the item count limits",
            )?;
            need(
                !a.unique_items || xs.iter().enumerate().all(|(i, x)| !xs[..i].contains(x)),
                "unique array items",
            )?;
            let Some(item) = &a.items else { return Ok(()) };
            xs.iter()
                .enumerate()
                .try_for_each(|(i, x)| check(spec, item, x, &format!("{at}[{i}]")))
        }
        (SchemaKind::Type(Type::Object(o)), Value::Object(m)) => {
            if let Some(k) = o.required.iter().find(|k| !m.contains_key(*k)) {
                return Err(format!("{at}.{k}: required field is missing"));
            }
            o.properties
                .iter()
                .filter_map(|(k, p)| Some((k, p, m.get(k)?)))
                .try_for_each(|(k, p, x)| check(spec, p, x, &format!("{at}.{k}")))
        }
        (SchemaKind::Type(t), _) => {
            let name = serde_json::to_value(t)
                .ok()
                .and_then(|t| Some(t["type"].as_str()?.to_string()));
            need(
                false,
                &format!("a value of type {}", name.unwrap_or_default()),
            )
        }
        (SchemaKind::OneOf { one_of: rs } | SchemaKind::AnyOf { any_of: rs }, _) => need(
            rs.iter().any(|r| check(spec, r, v, at).is_ok()),
            "a value matching one of the listed schemas",
        ),
        (SchemaKind::AllOf { all_of }, _) => all_of.iter().try_for_each(|r| check(spec, r, v, at)),
        (SchemaKind::Not { .. } | SchemaKind::Any(_), _) => Ok(()),
    }
}

fn word() -> &'static str {
    WORDS[random_range(0..WORDS.len())]
}

/// Picks a random enum value, or `None` when there is no enum.
fn pick<T: Clone + Into<Value>>(e: &[Option<T>]) -> Option<Value> {
    (!e.is_empty()).then(|| e[random_range(0..e.len())].clone().into())
}

fn allowed<T: PartialEq>(e: &[Option<T>], x: &T) -> bool {
    e.is_empty() || e.iter().any(|o| o.as_ref() == Some(x))
}

fn within(n: usize, lo: Option<usize>, hi: Option<usize>) -> bool {
    lo.is_none_or(|m| n >= m) && hi.is_none_or(|m| n <= m)
}
