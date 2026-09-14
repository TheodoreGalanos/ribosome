use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportProfile {
    pub profile_version: String,
    pub id: String,
    pub version: String,
    pub input: Input,
    pub assemble: Assembly,
    pub episode: EpisodeMapping,
    pub metadata: BTreeMap<String, String>,
    pub annotations: BTreeMap<String, String>,
    pub policy: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Input {
    pub kind: String,
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin_before_read: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_env: Option<String>,
    pub tables: Vec<Table>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    pub encoding: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows_pointer: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assembly {
    pub base: String,
    #[serde(rename = "as")]
    pub namespace: String,
    pub joins: Vec<Join>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Join {
    pub table: String,
    #[serde(rename = "as")]
    pub namespace: String,
    pub left: String,
    pub right: String,
    pub cardinality: String,
    pub missing: String,
    pub duplicates: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeMapping {
    pub id: String,
    pub task: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    pub decoder: Decoder,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decoder {
    pub name: String,
    pub field: String,
    pub value_encoding: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<Fallback>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fallback {
    pub name: String,
    pub field: String,
    pub value_encoding: String,
    pub on: Vec<String>,
}

impl ImportProfile {
    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        if text.len() > 256 * 1024 {
            return Err(Error::invalid("profile exceeds 256 KiB"));
        }
        let value: Value = serde_saphyr::from_str(&text)
            .map_err(|e| Error::invalid(format!("invalid YAML profile: {e}")))?;
        let schema: Value = serde_json::from_str(include_str!("../profile.schema.json"))?;
        let validator =
            jsonschema::validator_for(&schema).map_err(|e| Error::internal(e.to_string()))?;
        if let Err(e) = validator.validate(&value) {
            return Err(Error::invalid(format!("invalid import profile: {e}")));
        }
        let profile: Self = serde_json::from_value(value)?;
        profile.check()?;
        Ok(profile)
    }

    pub fn check(&self) -> Result<()> {
        let names: BTreeSet<_> = self.input.tables.iter().map(|t| t.name.as_str()).collect();
        if names.len() != self.input.tables.len() || !names.contains(self.assemble.base.as_str()) {
            return Err(Error::invalid(
                "table names must be unique and include the assembly base",
            ));
        }
        let mut namespaces = BTreeSet::from([self.assemble.namespace.as_str()]);
        for join in &self.assemble.joins {
            if !names.contains(join.table.as_str()) || !namespaces.insert(&join.namespace) {
                return Err(Error::invalid(
                    "joins must name existing tables and distinct namespaces",
                ));
            }
        }
        if self.input.kind == "huggingface" && self.input.mode != "pinned_files" {
            return Err(Error::invalid(
                "HF acquisition requires pinned_files; save viewer responses as a local snapshot",
            ));
        }
        for table in &self.input.tables {
            if self.input.kind == "local" && table.path.is_none() {
                return Err(Error::invalid("local tables require a path"));
            }
        }
        Ok(())
    }
}

pub fn identifier(row: &Value, pointer: &str) -> Result<String> {
    match row.pointer(pointer) {
        Some(Value::String(s)) if !s.is_empty() => Ok(s.clone()),
        Some(Value::Number(n)) => Ok(n.to_string()),
        _ => Err(Error::invalid(format!(
            "missing scalar identity at {pointer}"
        ))),
    }
}

pub fn project(row: &Value, mapping: &BTreeMap<String, String>) -> Value {
    Value::Object(
        mapping
            .iter()
            .filter_map(|(key, pointer)| {
                row.pointer(pointer)
                    .map(|value| (key.clone(), value.clone()))
            })
            .collect(),
    )
}
