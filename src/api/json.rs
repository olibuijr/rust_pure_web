//! Zero-dependency JSON parser and builder
use crate::db::{Value, Document};

// ── Parser ───────────────────────────────────────────────────────────────────

pub fn parse_json(input: &str) -> Document {
    let mut doc = Document::new();
    let input = input.trim();
    if !input.starts_with('{') { return doc; }
    
    // Very basic parser for flat objects (zero-dep)
    let inner = input.trim_start_matches('{').trim_end_matches('}');
    let mut parts = inner.split(',');
    
    while let Some(part) = parts.next() {
        if let Some((k, v)) = part.split_once(':') {
            let key = k.trim().trim_matches('"').to_string();
            let val_str = v.trim();
            
            let value = if val_str.starts_with('"') {
                Value::String(val_str.trim_matches('"').to_string())
            } else if val_str == "true" {
                Value::Bool(true)
            } else if val_str == "false" {
                Value::Bool(false)
            } else if let Ok(n) = val_str.parse::<i64>() {
                Value::Int(n)
            } else if let Ok(n) = val_str.parse::<f64>() {
                Value::Float(n)
            } else {
                Value::String(val_str.to_string())
            };
            
            doc.insert(key, value);
        }
    }
    doc
}

// ── Builder (New "Better Tool") ──────────────────────────────────────────────

pub struct JsonSerializer;

impl JsonSerializer {
    pub fn escape(s: &str) -> String {
        let mut escaped = String::with_capacity(s.len());
        for c in s.chars() {
            match c {
                '"' => escaped.push_str("\\\""),
                '\\' => escaped.push_str("\\\\"),
                '\n' => escaped.push_str("\\n"),
                '\r' => escaped.push_str("\\r"),
                '\t' => escaped.push_str("\\t"),
                _ => escaped.push(c),
            }
        }
        escaped
    }

    pub fn wrap_string(s: &str) -> String {
        format!("\"{}\"", Self::escape(s))
    }

    pub fn key_string(key: &str, value: &str) -> String {
        format!("\"{}\":\"{}\"", key, Self::escape(value))
    }
}
