#![allow(clippy::useless_conversion)] // Needed for rusqlite::ToSql trait
use crate::Error;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use rusqlite::types::{Null, ValueRef};
use rusqlite::ToSql;
use serde_json::Value as JsonValue;

/// Converts a JSON value into a `rusqlite::ToSql` compatible type.
/// Note: Does not support JSON Arrays or Objects as parameters.
pub(crate) fn json_to_rusqlite_param(value: JsonValue) -> Result<Box<dyn ToSql>, Error> {
    Ok(match value {
        JsonValue::Null => Box::new(Null),
        JsonValue::Bool(b) => Box::new(b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Box::new(i)
            } else if let Some(f) = n.as_f64() {
                Box::new(f)
            } else {
                return Err(Error::ValueConversionError(
                    "Unsupported number type".to_string(),
                ));
            }
        }
        JsonValue::String(s) => {
            // Guard against double-serialization: if the string looks like a
            // JSON-encoded string (starts and ends with `"`) and is valid JSON,
            // unwrap the inner value instead of storing the quoted form.
            //
            // This happens when a `String` or `&str` is passed through
            // `serde_json::to_value` twice — e.g. via the `params!` macro on a
            // value that was already a `serde_json::Value::String`.
            if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
                if let Ok(JsonValue::String(inner)) = serde_json::from_str::<JsonValue>(&s) {
                    return Ok(Box::new(inner));
                }
            }
            Box::new(s)
        }
        JsonValue::Array(_) => {
            return Err(Error::ValueConversionError(
                "JSON arrays are not supported as parameters".to_string(),
            ))
        }
        JsonValue::Object(_) => {
            return Err(Error::ValueConversionError(
                "JSON objects are not supported as parameters".to_string(),
            ))
        }
    })
}

/// Converts a vector of JSON values into a vector of `rusqlite::ToSql` boxed traits.
pub(crate) fn json_to_rusqlite_params(
    params: Vec<JsonValue>,
) -> Result<Vec<Box<dyn ToSql>>, Error> {
    params.into_iter().map(json_to_rusqlite_param).collect()
}

/// Converts a `rusqlite::types::ValueRef` into a `serde_json::Value`.
/// Blobs are encoded as base64 strings.
pub(crate) fn rusqlite_value_to_json(value_ref: ValueRef<'_>) -> Result<JsonValue, Error> {
    Ok(match value_ref {
        ValueRef::Null => JsonValue::Null,
        ValueRef::Integer(i) => JsonValue::Number(i.into()),
        ValueRef::Real(f) => {
            JsonValue::Number(serde_json::Number::from_f64(f).ok_or_else(|| {
                Error::ValueConversionError(format!("Cannot convert f64 '{}' to JSON Number", f))
            })?)
        }
        ValueRef::Text(t) => JsonValue::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => JsonValue::String(BASE64_STANDARD.encode(b)),
    })
}
