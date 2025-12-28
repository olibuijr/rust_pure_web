//! In-memory document database with encrypted file sync
use crate::crypto::{chacha20, random_bytes, random_hex, sha256};
use crate::{config, realtime};
use std::collections::HashMap;
use std::fs;
use std::sync::{RwLock, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const DB_VERSION: u8 = 1;

/// JSON-like value type
#[derive(Clone, Debug)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<Value>),
    Object(HashMap<String, Value>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self { Value::String(s) => Some(s), _ => None }
    }
    pub fn as_object(&self) -> Option<&HashMap<String, Value>> {
        match self { Value::Object(o) => Some(o), _ => None }
    }
}

pub type Document = HashMap<String, Value>;
pub type Collection = HashMap<String, Document>;

/// Database schema definition
#[derive(Clone)]
pub struct Schema {
    pub fields: Vec<(String, String)>, // (name, type)
}

/// The database
pub struct Database {
    collections: RwLock<HashMap<String, Collection>>,
    schemas: RwLock<HashMap<String, Schema>>,
    encryption_key: [u8; 32],
}

static DB: OnceLock<Database> = OnceLock::new();

impl Database {
    fn new(key: &[u8]) -> Self {
        let mut encryption_key = [0u8; 32];
        let hash = sha256(key);
        encryption_key.copy_from_slice(&hash);

        let db = Database {
            collections: RwLock::new(HashMap::new()),
            schemas: RwLock::new(HashMap::new()),
            encryption_key,
        };

        // Create default users collection
        db.create_collection_internal("_users", vec![
            ("email".into(), "string".into()),
            ("password".into(), "string".into()),
            ("role".into(), "string".into()),
            ("created".into(), "int".into()),
        ]);

        // Create sessions collection
        db.create_collection_internal("_sessions", vec![
            ("user_id".into(), "string".into()),
            ("token".into(), "string".into()),
            ("expires".into(), "int".into()),
        ]);

        // Create settings collection
        db.create_collection_internal("_settings", vec![
            ("page_title".into(), "string".into()),
            ("meta_description".into(), "string".into()),
            ("meta_keywords".into(), "string".into()),
            ("og_title".into(), "string".into()),
            ("og_description".into(), "string".into()),
            ("og_image".into(), "string".into()),
            ("twitter_card".into(), "string".into()),
            ("canonical_url".into(), "string".into()),
            ("nginx_hostname".into(), "string".into()),
            ("nginx_internal_ip".into(), "string".into()),
            ("dev_network_name".into(), "string".into()),
            ("dev_network_subnet".into(), "string".into()),
            ("dev_ip_base".into(), "string".into()),
            ("prod_network_name".into(), "string".into()),
            ("prod_network_subnet".into(), "string".into()),
            ("prod_ip_base".into(), "string".into()),
            ("app_port".into(), "int".into()),
            ("dev_port_start".into(), "int".into()),
            ("dev_port_end".into(), "int".into()),
            ("prod_port_start".into(), "int".into()),
            ("prod_port_end".into(), "int".into()),
        ]);

        // Create internal ports collection for project allocations
        db.create_collection_internal("_ports", vec![
            ("project".into(), "string".into()),
            ("dev_port".into(), "int".into()),
            ("prod_port".into(), "int".into()),
            ("created".into(), "int".into()),
        ]);

        db
    }

    pub fn create_collection(&self, name: &str, fields: Vec<(String, String)>) {
        self.create_collection_internal(name, fields);
        self.sync();
        broadcast_event("collection.created", name, None, None);
    }

    fn create_collection_internal(&self, name: &str, fields: Vec<(String, String)>) {
        let mut cols = self.collections.write().unwrap();
        let mut schemas = self.schemas.write().unwrap();
        cols.insert(name.to_string(), HashMap::new());
        schemas.insert(name.to_string(), Schema { fields });
    }

    pub fn list_collections(&self) -> Vec<String> {
        self.schemas
            .read()
            .unwrap()
            .keys()
            .filter(|k| !k.starts_with('_'))
            .cloned()
            .collect()
    }

    pub fn list_all_collections(&self) -> Vec<String> {
        self.schemas.read().unwrap().keys().cloned().collect()
    }

    pub fn insert(&self, collection: &str, doc: Document) -> Option<String> {
        let mut cols = self.collections.write().unwrap();
        let col = cols.get_mut(collection)?;
        let id = random_hex(12);
        let mut doc = doc;
        doc.insert("id".into(), Value::String(id.clone()));
        doc.insert("created".into(), Value::Int(now()));
        doc.insert("updated".into(), Value::Int(now()));
        col.insert(id.clone(), doc);
        drop(cols);
        self.sync();
        if let Some(doc) = self.find_one(collection, &id) {
            broadcast_event("doc.created", collection, Some(&doc), Some(&id));
        }
        Some(id)
    }

    pub fn find_one(&self, collection: &str, id: &str) -> Option<Document> {
        self.collections.read().unwrap().get(collection)?.get(id).cloned()
    }

    pub fn find_by(&self, collection: &str, field: &str, value: &str) -> Option<Document> {
        let cols = self.collections.read().unwrap();
        let col = cols.get(collection)?;
        col.values().find(|doc| {
            doc.get(field).and_then(|v| v.as_str()) == Some(value)
        }).cloned()
    }

    pub fn find_all(&self, collection: &str) -> Vec<Document> {
        self.collections.read().unwrap().get(collection)
            .map(|c| c.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn update(&self, collection: &str, id: &str, updates: Document) -> bool {
        let mut cols = self.collections.write().unwrap();
        if let Some(col) = cols.get_mut(collection) {
            if let Some(doc) = col.get_mut(id) {
                for (k, v) in updates {
                    if k != "id" && k != "created" {
                        doc.insert(k, v);
                    }
                }
                doc.insert("updated".into(), Value::Int(now()));
                drop(cols);
                self.sync();
                if let Some(doc) = self.find_one(collection, id) {
                    broadcast_event("doc.updated", collection, Some(&doc), Some(id));
                }
                return true;
            }
        }
        false
    }

    pub fn delete(&self, collection: &str, id: &str) -> bool {
        let mut cols = self.collections.write().unwrap();
        if let Some(col) = cols.get_mut(collection) {
            if col.remove(id).is_some() {
                drop(cols);
                self.sync();
                broadcast_event("doc.deleted", collection, None, Some(id));
                return true;
            }
        }
        false
    }

    pub fn delete_collection(&self, name: &str) -> bool {
        if name.starts_with('_') { return false; }
        let mut cols = self.collections.write().unwrap();
        let mut schemas = self.schemas.write().unwrap();
        cols.remove(name);
        schemas.remove(name);
        drop(cols);
        drop(schemas);
        self.sync();
        broadcast_event("collection.deleted", name, None, None);
        true
    }

    /// Serialize database to binary
    fn serialize(&self) -> Vec<u8> {
        let cols = self.collections.read().unwrap();
        let schemas = self.schemas.read().unwrap();
        let mut data = Vec::new();

        // Write schemas
        data.extend(&(schemas.len() as u32).to_le_bytes());
        for (name, schema) in schemas.iter() {
            write_string(&mut data, name);
            data.extend(&(schema.fields.len() as u32).to_le_bytes());
            for (fname, ftype) in &schema.fields {
                write_string(&mut data, fname);
                write_string(&mut data, ftype);
            }
        }

        // Write collections
        for (name, col) in cols.iter() {
            write_string(&mut data, name);
            data.extend(&(col.len() as u32).to_le_bytes());
            for (id, doc) in col.iter() {
                write_string(&mut data, id);
                write_doc(&mut data, doc);
            }
        }

        data
    }

    /// Deserialize database from binary
    fn deserialize(&self, data: &[u8]) {
        let mut pos = 0;
        let mut schemas = self.schemas.write().unwrap();
        let mut cols = self.collections.write().unwrap();

        // Read schemas
        let schema_count = read_u32(data, &mut pos);
        for _ in 0..schema_count {
            let name = read_string(data, &mut pos);
            let field_count = read_u32(data, &mut pos);
            let mut fields = Vec::new();
            for _ in 0..field_count {
                let fname = read_string(data, &mut pos);
                let ftype = read_string(data, &mut pos);
                fields.push((fname, ftype));
            }
            schemas.insert(name.clone(), Schema { fields });
            cols.insert(name, HashMap::new());
        }

        // Read collections
        while pos < data.len() {
            let name = read_string(data, &mut pos);
            let doc_count = read_u32(data, &mut pos);
            let col = cols.entry(name).or_insert_with(HashMap::new);
            for _ in 0..doc_count {
                let id = read_string(data, &mut pos);
                let doc = read_doc(data, &mut pos);
                col.insert(id, doc);
            }
        }
    }

    /// Sync to encrypted file
    fn sync(&self) {
        let data = self.serialize();
        let nonce: [u8; 12] = random_bytes(12).try_into().unwrap_or([0; 12]);
        let encrypted = chacha20(&self.encryption_key, &nonce, &data);

        let mut file_data = vec![DB_VERSION];
        file_data.extend_from_slice(&nonce);
        file_data.extend(encrypted);

        let data_dir = config::data_dir();
        let _ = fs::create_dir_all(&data_dir);
        let _ = fs::write(db_path(), &file_data);
    }

    /// Load from encrypted file
    fn load(&self) {
        if let Ok(file_data) = fs::read(db_path()) {
            if file_data.len() < 14 || file_data[0] != DB_VERSION { return; }
            let nonce: [u8; 12] = file_data[1..13].try_into().unwrap();
            let decrypted = chacha20(&self.encryption_key, &nonce, &file_data[13..]);
            self.deserialize(&decrypted);
            self.migrate_system_defaults();
        }
    }

    /// Create backup
    pub fn backup(&self) -> String {
        let timestamp = now();
        let backup_path = config::data_dir().join(format!("backup_{}.bin", timestamp));
        let _ = fs::copy(db_path(), &backup_path);
        backup_path.to_string_lossy().to_string()
    }

    fn migrate_system_defaults(&self) {
        self.ensure_internal_collections();
        self.ensure_settings_defaults();
    }

    fn ensure_internal_collections(&self) {
        let mut cols = self.collections.write().unwrap();
        let mut schemas = self.schemas.write().unwrap();
        if !schemas.contains_key("_ports") {
            cols.insert("_ports".to_string(), HashMap::new());
            schemas.insert("_ports".to_string(), Schema {
                fields: vec![
                    ("project".into(), "string".into()),
                    ("dev_port".into(), "int".into()),
                    ("prod_port".into(), "int".into()),
                    ("created".into(), "int".into()),
                ],
            });
        }
    }

    fn ensure_settings_defaults(&self) {
        let mut cols = self.collections.write().unwrap();
        let col = cols.entry("_settings".to_string()).or_insert_with(HashMap::new);
        if col.is_empty() {
            let mut doc = Document::new();
            doc.insert("page_title".into(), Value::String("Rust Pure Web".into()));
            doc.insert("meta_description".into(), Value::String("Zero-dependency Rust web framework.".into()));
            doc.insert("meta_keywords".into(), Value::String("rust, web, zero-deps".into()));
            doc.insert("og_title".into(), Value::String("Rust Pure Web".into()));
            doc.insert("og_description".into(), Value::String("Fast, zero-dependency web framework in Rust.".into()));
            doc.insert("og_image".into(), Value::String("".into()));
            doc.insert("twitter_card".into(), Value::String("summary_large_image".into()));
            doc.insert("canonical_url".into(), Value::String("".into()));
            doc.insert("nginx_hostname".into(), Value::String("proxy.olibuijr.com".into()));
            doc.insert("nginx_internal_ip".into(), Value::String("192.168.8.4".into()));
            doc.insert("dev_network_name".into(), Value::String("dev".into()));
            doc.insert("dev_network_subnet".into(), Value::String("10.35.0.0/24".into()));
            doc.insert("dev_ip_base".into(), Value::String("10.35.0.".into()));
            doc.insert("prod_network_name".into(), Value::String("prod".into()));
            doc.insert("prod_network_subnet".into(), Value::String("10.36.0.0/24".into()));
            doc.insert("prod_ip_base".into(), Value::String("10.36.0.".into()));
            doc.insert("app_port".into(), Value::Int(3460));
            doc.insert("dev_port_start".into(), Value::Int(3501));
            doc.insert("dev_port_end".into(), Value::Int(3599));
            doc.insert("prod_port_start".into(), Value::Int(3601));
            doc.insert("prod_port_end".into(), Value::Int(3699));
            let id = random_hex(12);
            col.insert(id, doc);
            return;
        }

        for (_id, doc) in col.iter_mut() {
            set_default(doc, "page_title", Value::String("Rust Pure Web".into()));
            set_default(doc, "meta_description", Value::String("Zero-dependency Rust web framework.".into()));
            set_default(doc, "meta_keywords", Value::String("rust, web, zero-deps".into()));
            set_default(doc, "og_title", Value::String("Rust Pure Web".into()));
            set_default(doc, "og_description", Value::String("Fast, zero-dependency web framework in Rust.".into()));
            set_default(doc, "og_image", Value::String("".into()));
            set_default(doc, "twitter_card", Value::String("summary_large_image".into()));
            set_default(doc, "canonical_url", Value::String("".into()));
            set_default(doc, "nginx_hostname", Value::String("proxy.olibuijr.com".into()));
            set_default(doc, "nginx_internal_ip", Value::String("192.168.8.4".into()));
            set_default(doc, "dev_network_name", Value::String("dev".into()));
            set_default(doc, "dev_network_subnet", Value::String("10.35.0.0/24".into()));
            set_default(doc, "dev_ip_base", Value::String("10.35.0.".into()));
            set_default(doc, "prod_network_name", Value::String("prod".into()));
            set_default(doc, "prod_network_subnet", Value::String("10.36.0.0/24".into()));
            set_default(doc, "prod_ip_base", Value::String("10.36.0.".into()));
            set_default(doc, "app_port", Value::Int(3460));
            set_default(doc, "dev_port_start", Value::Int(3501));
            set_default(doc, "dev_port_end", Value::Int(3599));
            set_default(doc, "prod_port_start", Value::Int(3601));
            set_default(doc, "prod_port_end", Value::Int(3699));
        }
    }

}

fn db_path() -> std::path::PathBuf {
    config::data_dir().join("db.bin")
}

fn broadcast_event(kind: &str, collection: &str, doc: Option<&Document>, id: Option<&str>) {
    let mut payload = Vec::new();
    payload.push(format!(r#""type":"{}""#, kind));
    payload.push(format!(r#""collection":"{}""#, collection));
    if let Some(id) = id {
        payload.push(format!(r#""id":"{}""#, id));
    }
    if let Some(doc) = doc {
        let doc_json = doc_to_json_for_collection(collection, doc);
        payload.push(format!(r#""doc":{}"#, doc_json));
    }
    let json = format!("{{{}}}", payload.join(","));
    realtime::broadcast(&json);
}

// Binary helpers
fn write_string(data: &mut Vec<u8>, s: &str) {
    data.extend(&(s.len() as u32).to_le_bytes());
    data.extend(s.as_bytes());
}

fn write_doc(data: &mut Vec<u8>, doc: &Document) {
    data.extend(&(doc.len() as u32).to_le_bytes());
    for (k, v) in doc {
        write_string(data, k);
        write_value(data, v);
    }
}

fn write_value(data: &mut Vec<u8>, v: &Value) {
    match v {
        Value::Null => data.push(0),
        Value::Bool(b) => { data.push(1); data.push(if *b { 1 } else { 0 }); }
        Value::Int(i) => { data.push(2); data.extend(&i.to_le_bytes()); }
        Value::Float(f) => { data.push(3); data.extend(&f.to_le_bytes()); }
        Value::String(s) => { data.push(4); write_string(data, s); }
        Value::Array(arr) => {
            data.push(5);
            data.extend(&(arr.len() as u32).to_le_bytes());
            for item in arr { write_value(data, item); }
        }
        Value::Object(obj) => { data.push(6); write_doc(data, obj); }
    }
}

fn set_default(doc: &mut Document, key: &str, value: Value) {
    if !doc.contains_key(key) {
        doc.insert(key.to_string(), value);
    }
}

fn read_u32(data: &[u8], pos: &mut usize) -> u32 {
    let val = u32::from_le_bytes(data[*pos..*pos+4].try_into().unwrap_or([0;4]));
    *pos += 4;
    val
}

fn read_string(data: &[u8], pos: &mut usize) -> String {
    let len = read_u32(data, pos) as usize;
    let s = String::from_utf8_lossy(&data[*pos..*pos+len]).to_string();
    *pos += len;
    s
}

fn read_doc(data: &[u8], pos: &mut usize) -> Document {
    let count = read_u32(data, pos);
    let mut doc = HashMap::new();
    for _ in 0..count {
        let k = read_string(data, pos);
        let v = read_value(data, pos);
        doc.insert(k, v);
    }
    doc
}

fn read_value(data: &[u8], pos: &mut usize) -> Value {
    let tag = data[*pos];
    *pos += 1;
    match tag {
        0 => Value::Null,
        1 => { let b = data[*pos] != 0; *pos += 1; Value::Bool(b) }
        2 => { let v = i64::from_le_bytes(data[*pos..*pos+8].try_into().unwrap_or([0;8])); *pos += 8; Value::Int(v) }
        3 => { let v = f64::from_le_bytes(data[*pos..*pos+8].try_into().unwrap_or([0;8])); *pos += 8; Value::Float(v) }
        4 => Value::String(read_string(data, pos)),
        5 => {
            let count = read_u32(data, pos);
            let mut arr = Vec::new();
            for _ in 0..count { arr.push(read_value(data, pos)); }
            Value::Array(arr)
        }
        6 => Value::Object(read_doc(data, pos)),
        _ => Value::Null,
    }
}

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Initialize database with encryption key
pub fn init(key: &str) {
    let db = DB.get_or_init(|| Database::new(key.as_bytes()));
    db.load();
}

/// Get database reference
pub fn get() -> &'static Database {
    DB.get().expect("Database not initialized")
}

pub fn doc_to_json_for_collection(collection: &str, doc: &Document) -> String {
    let pairs: Vec<String> = doc.iter().filter_map(|(k, v)| {
        if collection == "_users" && k == "password" {
            return None;
        }
        Some(format!(r#""{}": {}"#, k, value_to_json(v)))
    }).collect();
    format!("{{{}}}", pairs.join(", "))
}

pub fn value_to_json(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => if *b { "true" } else { "false" }.into(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => format!(r#""{}""#, s.replace('\\', "\\\\").replace('"', "\\\"")),
        Value::Array(arr) => format!("[{}]", arr.iter().map(value_to_json).collect::<Vec<_>>().join(", ")),
        Value::Object(obj) => doc_to_json_for_collection("", obj),
    }
}
