//! Preview of what omicron's sled-agent inventory will collect from FMD.
//!
//! This mirrors the conversion in omicron's `sled-agent/src/fmd.rs`:
//! cases are collected with their full event nvlists serialized to JSON,
//! and resources are collected with their fault status flags.

use fmd_adm::{FmdAdm, NvList, NvValue};

fn nvvalue_to_json(value: &NvValue) -> serde_json::Value {
    match value {
        NvValue::Boolean => serde_json::Value::Bool(true),
        NvValue::BooleanValue(b) => serde_json::Value::Bool(*b),
        NvValue::Byte(n) => serde_json::json!(*n),
        NvValue::Int8(n) => serde_json::json!(*n),
        NvValue::UInt8(n) => serde_json::json!(*n),
        NvValue::Int16(n) => serde_json::json!(*n),
        NvValue::UInt16(n) => serde_json::json!(*n),
        NvValue::Int32(n) => serde_json::json!(*n),
        NvValue::UInt32(n) => serde_json::json!(*n),
        NvValue::Int64(n) => serde_json::json!(*n),
        NvValue::UInt64(n) => serde_json::json!(*n),
        NvValue::Double(f) => serde_json::json!(*f),
        NvValue::String(s) => serde_json::Value::String(s.clone()),
        NvValue::Hrtime(n) => serde_json::json!(*n),
        NvValue::NvList(nvl) => nvlist_to_json(nvl),
        NvValue::BooleanArray(arr) => serde_json::json!(arr),
        NvValue::ByteArray(arr) => serde_json::json!(arr),
        NvValue::Int8Array(arr) => serde_json::json!(arr),
        NvValue::UInt8Array(arr) => serde_json::json!(arr),
        NvValue::Int16Array(arr) => serde_json::json!(arr),
        NvValue::UInt16Array(arr) => serde_json::json!(arr),
        NvValue::Int32Array(arr) => serde_json::json!(arr),
        NvValue::UInt32Array(arr) => serde_json::json!(arr),
        NvValue::Int64Array(arr) => serde_json::json!(arr),
        NvValue::UInt64Array(arr) => serde_json::json!(arr),
        NvValue::StringArray(arr) => serde_json::json!(arr),
        NvValue::NvListArray(arr) => {
            let items: Vec<serde_json::Value> = arr.iter().map(nvlist_to_json).collect();
            serde_json::Value::Array(items)
        }
        NvValue::Unknown { type_code } => {
            serde_json::json!({
                "_unknown_type": format!("{type_code:?}")
            })
        }
    }
}

fn nvlist_to_json(nvl: &NvList) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (name, value) in nvl {
        map.insert(name.to_string(), nvvalue_to_json(value));
    }
    serde_json::Value::Object(map)
}

fn main() {
    let adm = FmdAdm::open().expect("failed to open fmd");

    // Collect cases — same as omicron's FmdCase
    let cases = adm.cases(None).expect("failed to list cases");
    let cases_json: Vec<serde_json::Value> = cases
        .iter()
        .map(|c| {
            serde_json::json!({
                "uuid": c.uuid.to_string(),
                "code": &c.code,
                "url": &c.url,
                "event": c.event.as_ref().map(nvlist_to_json),
            })
        })
        .collect();

    // Collect resources — same as omicron's FmdResource
    let resources = adm.resources(true).expect("failed to list resources");
    let resources_json: Vec<serde_json::Value> = resources
        .iter()
        .map(|r| {
            serde_json::json!({
                "fmri": &r.fmri,
                "uuid": r.uuid.to_string(),
                "case_id": r.case.to_string(),
                "faulty": r.faulty,
                "unusable": r.unusable,
                "invisible": r.invisible,
            })
        })
        .collect();

    // This is what the sled-agent API would return
    let inventory = serde_json::json!({
        "type": "available",
        "value": {
            "cases": cases_json,
            "resources": resources_json,
        }
    });

    println!("{}", serde_json::to_string_pretty(&inventory).unwrap());
}
