use crate::{Error, Limits, Result, profile::Decoder};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub source_index: usize,
    pub role: String,
    pub content: Value,
    pub tool_calls: Vec<Value>,
    pub tool_call_id: Option<String>,
    pub name: Option<String>,
    pub timestamp: Option<Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Coverage {
    pub messages: usize,
    pub tool_calls: usize,
    pub paired_results: usize,
    pub pending_calls: Vec<String>,
    pub unmatched_results: Vec<String>,
    pub original_timestamps: usize,
    pub opaque_parts: usize,
    pub modalities: Vec<String>,
    pub prefix_safe: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decoded {
    pub decoder: String,
    pub fallback_reason: Option<String>,
    pub messages: Vec<Message>,
    pub coverage: Coverage,
    pub limitations: Vec<String>,
}

enum Parsed {
    Empty(&'static str),
    Messages(Vec<Value>),
}

fn parse(value: Option<&Value>, encoding: &str, decoder: &str) -> Result<Parsed> {
    let Some(value) = value else {
        return Ok(Parsed::Empty("missing"));
    };
    if value.is_null() {
        return Ok(Parsed::Empty("null"));
    }
    if value.as_str().is_some_and(|s| s.trim().is_empty())
        || value.as_array().is_some_and(Vec::is_empty)
    {
        return Ok(Parsed::Empty("empty"));
    }
    let mut rows = match encoding {
        "array" => value
            .as_array()
            .cloned()
            .ok_or_else(|| Error::invalid("message field must be an array"))?,
        "json_string" => serde_json::from_str::<Vec<Value>>(
            value
                .as_str()
                .ok_or_else(|| Error::invalid("encoded messages must be a string"))?,
        )?,
        "jsonl_string" => value
            .as_str()
            .ok_or_else(|| Error::invalid("encoded JSONL must be a string"))?
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty())
            .map(|(line, text)| {
                serde_json::from_str(text).map_err(|e| {
                    Error::invalid(format!(
                        "malformed embedded JSONL at line {}: {e}",
                        line + 1
                    ))
                })
            })
            .collect::<Result<Vec<Value>>>()?,
        _ => return Err(Error::invalid("unsupported message encoding")),
    };
    if decoder == "aec-jsonl-v1" {
        let header = rows.first().filter(|v| v.get("format").is_some());
        if let Some(header) = header {
            if header["format"] != "aec-bench-trajectory" || header["version"] != 1 {
                return Err(Error::invalid("unsupported AEC trajectory header"));
            }
            rows.remove(0);
            if rows.is_empty() {
                return Ok(Parsed::Empty("header_only"));
            }
        }
    }
    if rows.is_empty() {
        Ok(Parsed::Empty("empty"))
    } else {
        Ok(Parsed::Messages(rows))
    }
}

pub fn decode(row: &Value, decoder: &Decoder, limits: &Limits) -> Result<Decoded> {
    let (rows, name, fallback_reason) = match parse(
        row.pointer(&decoder.field),
        &decoder.value_encoding,
        &decoder.name,
    )? {
        Parsed::Messages(rows) => (rows, decoder.name.clone(), None),
        Parsed::Empty(reason) => {
            let fallback = decoder
                .fallback
                .as_ref()
                .filter(|f| f.on.iter().any(|r| r == reason))
                .ok_or_else(|| Error::invalid(format!("trajectory is {reason}")))?;
            match parse(
                row.pointer(&fallback.field),
                &fallback.value_encoding,
                &fallback.name,
            )? {
                Parsed::Messages(rows) => (rows, fallback.name.clone(), Some(reason.into())),
                Parsed::Empty(why) => {
                    return Err(Error::invalid(format!("fallback trajectory is {why}")));
                }
            }
        }
    };
    if rows.len() > limits.messages {
        return Err(Error::invalid("episode exceeds message limit"));
    }
    let mut messages = Vec::new();
    let mut limitations = BTreeSet::new();
    let mut pending = BTreeMap::new();
    let mut seen_calls = BTreeSet::new();
    let mut unmatched = Vec::new();
    let mut paired = 0;
    let mut opaque = 0;
    let mut modalities = BTreeSet::new();
    for (index, row) in rows.into_iter().enumerate() {
        let object = row
            .as_object()
            .ok_or_else(|| Error::invalid(format!("message {index} is not an object")))?;
        let role = object
            .get("role")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::invalid(format!("message {index} has no string role")))?
            .to_owned();
        let known_role =
            ["system", "developer", "user", "assistant", "tool"].contains(&role.as_str());
        if !known_role {
            limitations.insert(format!("message {index}: unclassified role {role}"));
            opaque += 1;
        }
        let content = match object.get("content") {
            None | Some(Value::Null) => Value::Null,
            Some(Value::String(text)) if known_role => { modalities.insert("text".into()); Value::String(text.clone()) },
            Some(Value::Array(parts)) if known_role => Value::Array(parts.iter().map(|part| {
                if part["type"] == "text" && part["text"].is_string() {
                    modalities.insert("text".into()); json!({"type":"text", "text":part["text"]})
                } else {
                    opaque += 1;
                    limitations.insert(format!("message {index}: unsupported content retained owner-side"));
                    json!({"type":"opaque", "source_type":part.get("type").and_then(Value::as_str).unwrap_or("unknown")})
                }
            }).collect()),
            Some(_) => { opaque += 1; json!({"type":"opaque", "reason":"unclassified content retained owner-side"}) },
        };
        let calls = match object.get("tool_calls") {
            None | Some(Value::Null) => vec![],
            Some(Value::Array(calls)) => calls.clone(),
            _ => {
                return Err(Error::invalid(format!(
                    "message {index}: tool_calls is not an array"
                )));
            }
        };
        let mut projected_calls = Vec::new();
        for (number, call) in calls.iter().enumerate() {
            let call_id = call["id"].as_str().map(str::to_owned);
            let label = call_id
                .clone()
                .unwrap_or_else(|| format!("unidentified:{index}:{number}"));
            if !seen_calls.insert(label.clone()) {
                return Err(Error::invalid(format!(
                    "duplicate tool call identity {label}"
                )));
            }
            if call_id.is_none() {
                limitations.insert("some tool calls have no source identity".into());
            }
            pending.insert(label, index);
            let function = call.get("function").unwrap_or(call);
            let arguments = function.get("arguments").cloned().unwrap_or(Value::Null);
            let parsed = match arguments.as_str() {
                Some(raw) => serde_json::from_str::<Value>(raw)
                    .ok()
                    .filter(Value::is_object),
                None if arguments.is_object() => Some(arguments.clone()),
                _ => None,
            };
            if parsed.is_none() {
                limitations.insert(format!(
                    "message {index}: tool arguments preserved without a parsed object"
                ));
            }
            projected_calls.push(json!({"id":call_id,"name":function.get("name").and_then(Value::as_str),"arguments":arguments,"parsed_arguments":parsed}));
        }
        let tool_call_id = object
            .get("tool_call_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if role == "tool" {
            match tool_call_id.as_ref().and_then(|id| pending.remove(id)) {
                Some(_) => paired += 1,
                None => unmatched.push(
                    tool_call_id
                        .clone()
                        .unwrap_or_else(|| format!("unidentified-result:{index}")),
                ),
            }
        }
        messages.push(Message {
            source_index: index,
            role,
            content,
            tool_calls: projected_calls,
            tool_call_id,
            name: object
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned),
            timestamp: object.get("timestamp").filter(|v| !v.is_null()).cloned(),
        });
    }
    let timestamps = messages.iter().filter(|m| m.timestamp.is_some()).count();
    if timestamps < messages.len() {
        limitations
            .insert("Execution time is unknown for messages without an original timestamp.".into());
    }
    if !pending.is_empty() {
        limitations.insert("Some recorded tool calls have no paired result.".into());
    }
    if !unmatched.is_empty() {
        limitations
            .insert("Some recorded tool results have no identifiable preceding call.".into());
    }
    let coverage = Coverage {
        messages: messages.len(),
        tool_calls: seen_calls.len(),
        paired_results: paired,
        pending_calls: pending.into_keys().collect(),
        unmatched_results: unmatched,
        original_timestamps: timestamps,
        opaque_parts: opaque,
        modalities: modalities.into_iter().collect(),
        prefix_safe: true,
    };
    Ok(Decoded {
        decoder: name,
        fallback_reason,
        messages,
        coverage,
        limitations: limitations.into_iter().collect(),
    })
}
