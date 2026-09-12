use crate::error::{Error, Result};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::OnceLock};

pub const MAX_FRAME: usize = 1_048_576;
pub const MAX_PENDING: usize = 32;
pub const PROTOCOL: &str = "ribosome/1";
pub const PI_VERSION: &str = "0.85.1";

pub(crate) fn split_level(split: &crate::contracts::Split) -> u8 {
    use crate::contracts::Split;
    match split {
        Split::Development => 0,
        Split::Evaluation => 1,
        Split::Holdout => 2,
    }
}

pub(crate) fn derived_split(splits: &[crate::contracts::Split]) -> crate::contracts::Split {
    splits
        .iter()
        .max_by_key(|split| split_level(split))
        .cloned()
        .unwrap_or(crate::contracts::Split::Holdout)
}

pub fn schema() -> &'static Value {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        serde_json::from_str(include_str!("../../../contracts/schema.json"))
            .expect("built-in contract schema")
    })
}

pub fn validate(name: &str, value: &Value) -> Result<()> {
    static VALIDATORS: OnceLock<HashMap<String, jsonschema::Validator>> = OnceLock::new();
    let validators = VALIDATORS.get_or_init(|| schema()["$defs"].as_object().expect("definitions").keys().map(|name| {
        let source = json!({"$schema":"http://json-schema.org/draft-07/schema#", "$defs":schema()["$defs"], "$ref":format!("#/$defs/{name}")});
        (name.clone(), jsonschema::validator_for(&source).expect("valid built-in schema"))
    }).collect());
    let validator = validators
        .get(name)
        .ok_or_else(|| Error::invalid("unknown contract"))?;
    if let Some(error) = validator.iter_errors(value).next() {
        // Do not echo untrusted payloads or secrets into diagnostics.
        return Err(Error::invalid(format!(
            "{name}: invalid value at {}",
            error.instance_path()
        )));
    }
    Ok(())
}

pub fn decode<T: DeserializeOwned>(name: &str, value: Value) -> Result<T> {
    validate(name, &value)?;
    Ok(serde_json::from_value(value)?)
}

pub fn encode<T: Serialize>(name: &str, value: T) -> Result<Value> {
    let value = serde_json::to_value(value)?;
    validate(name, &value)?;
    Ok(value)
}

pub fn method_contract(method: &str) -> Result<(&str, &str)> {
    let entry = &schema()["x-methods"][method];
    match (entry[0].as_str(), entry[1].as_str()) {
        (Some(input), Some(output)) => Ok((input, output)),
        _ => Err(Error {
            code: -32601,
            message: "method not found".into(),
        }),
    }
}

pub fn counter(value: &str) -> Result<u64> {
    value
        .parse()
        .map_err(|_| Error::invalid("counter exceeds u64"))
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_millis() as u64
}

pub fn id() -> String {
    uuid::Uuid::now_v7().to_string()
}
