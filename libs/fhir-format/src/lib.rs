//! FHIR JSON ↔ XML conversion helpers.
//! The implementation is schema‑agnostic but follows the official
//! JSON/XML mapping rules used by HL7 FHIR:
//! - Root element uses the `resourceType` name.
//! - Primitive values are encoded with the `value` attribute.
//! - Primitive metadata (`id`, `extension`) is carried through `_field` entries.
//! - Arrays are represented by repeated elements and aligned metadata arrays.

use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::Writer;
use roxmltree::Document;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::io::Cursor;
use thiserror::Error;

const FHIR_NS: &str = "http://hl7.org/fhir";
const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";

#[derive(Debug, Error)]
pub enum FormatError {
    #[error("expected a JSON object for the resource")]
    ExpectedObject,
    #[error("missing resourceType property")]
    MissingResourceType,
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("XML parse error: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("UTF-8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("XML write error: {0}")]
    XmlWrite(#[from] quick_xml::Error),
}

/// Convert a FHIR JSON payload into its XML representation.
pub fn json_to_xml(input: &str) -> Result<String, FormatError> {
    let value: Value = serde_json::from_str(input)?;
    let obj = value.as_object().ok_or(FormatError::ExpectedObject)?;
    let resource_type = obj
        .get("resourceType")
        .and_then(Value::as_str)
        .ok_or(FormatError::MissingResourceType)?;

    let mut writer = Writer::new_with_indent(Cursor::new(Vec::new()), b' ', 2);
    let mut root = BytesStart::new(resource_type);
    root.push_attribute(("xmlns", FHIR_NS));
    writer.write_event(Event::Start(root.clone()))?;

    let mut meta = HashMap::new();
    for (k, v) in obj {
        if k.starts_with('_') {
            meta.insert(k.trim_start_matches('_').to_string(), v.clone());
        }
    }

    for (k, v) in obj {
        if k == "resourceType" || k.starts_with('_') {
            continue;
        }
        let meta_entry = meta.get(k);
        write_json_value(&mut writer, k, v, meta_entry)?;
    }

    // Handle metadata fields that don't have a corresponding value field
    // (e.g., _active with extensions but no active field)
    for (k, v) in &meta {
        if !obj.contains_key(k) {
            // This metadata has no corresponding value, write it as a primitive with no value
            write_json_value(&mut writer, k, &Value::Null, Some(v))?;
        }
    }

    writer.write_event(Event::End(BytesEnd::new(resource_type)))?;
    let bytes = writer.into_inner().into_inner();
    Ok(String::from_utf8(bytes)?)
}

/// Convert a FHIR XML payload into its JSON representation.
pub fn xml_to_json(input: &str) -> Result<String, FormatError> {
    let doc = Document::parse(input)?;
    let root = doc.root_element();

    let mut map = Map::new();
    map.insert(
        "resourceType".to_string(),
        Value::String(root.tag_name().name().to_string()),
    );

    let mut accumulator = Map::new();
    for child in root.children().filter(|n| n.is_element()) {
        process_xml_child(input, &mut accumulator, &child)?;
    }

    map.extend(accumulator);
    let json = Value::Object(map);
    Ok(serde_json::to_string_pretty(&json)?)
}

fn write_json_value(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    name: &str,
    value: &Value,
    meta: Option<&Value>,
) -> Result<(), FormatError> {
    match value {
        Value::Array(items) => {
            let meta_array = meta.and_then(Value::as_array);
            for (idx, item) in items.iter().enumerate() {
                let item_meta = meta_array.and_then(|m| m.get(idx));
                write_json_value(writer, name, item, item_meta)?;
            }
        }
        Value::Object(obj) => write_complex(writer, name, obj)?,
        Value::Null => {}
        primitive => write_primitive(writer, name, primitive, meta)?,
    }
    Ok(())
}

fn write_complex(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    name: &str,
    obj: &Map<String, Value>,
) -> Result<(), FormatError> {
    let mut meta = HashMap::new();
    for (k, v) in obj {
        if k.starts_with('_') {
            meta.insert(k.trim_start_matches('_').to_string(), v.clone());
        }
    }

    let mut start = BytesStart::new(name);
    if let Some(Value::String(id)) = obj.get("id") {
        start.push_attribute(("id", id.as_str()));
    }

    writer.write_event(Event::Start(start))?;

    for (k, v) in obj {
        if k.starts_with('_') || k == "id" {
            continue;
        }
        let meta_entry = meta.get(k);
        write_json_value(writer, k, v, meta_entry)?;
    }

    writer.write_event(Event::End(BytesEnd::new(name)))?;
    Ok(())
}

fn write_primitive(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    name: &str,
    value: &Value,
    meta: Option<&Value>,
) -> Result<(), FormatError> {
    let mut elem = BytesStart::new(name);

    // Only add value attribute if the value is not null
    let has_value = !matches!(value, Value::Null);
    if has_value {
        elem.push_attribute(("value", primitive_to_string(value).as_str()));
    }

    let mut has_children = false;
    if let Some(Value::Object(m)) = meta {
        if let Some(Value::String(id)) = m.get("id") {
            elem.push_attribute(("id", id.as_str()));
        }
        if m.get("extension").is_some() {
            has_children = true;
        }
    }

    // If we have neither a value nor children, skip writing this element
    if !has_value && !has_children {
        return Ok(());
    }

    if has_children {
        writer.write_event(Event::Start(elem.clone()))?;
        if let Some(Value::Object(m)) = meta {
            if let Some(ext) = m.get("extension") {
                write_json_value(writer, "extension", ext, None)?;
            }
        }
        writer.write_event(Event::End(BytesEnd::new(name)))?;
    } else {
        writer.write_event(Event::Empty(elem))?;
    }
    Ok(())
}

fn primitive_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "".to_string(),
        other => other.to_string(),
    }
}

fn process_xml_child(
    source: &str,
    target: &mut Map<String, Value>,
    node: &roxmltree::Node,
) -> Result<(), FormatError> {
    let name = node.tag_name().name().to_string();
    let (value, meta) = xml_element_to_value(source, node)?;

    insert_json_property(target, &name, value, meta);
    Ok(())
}

fn xml_element_to_value(
    source: &str,
    node: &roxmltree::Node,
) -> Result<(Value, Option<Value>), FormatError> {
    if node.tag_name().namespace().is_some_and(|ns| ns == XHTML_NS) {
        let snippet = &source[node.range()];
        return Ok((Value::String(snippet.to_string()), None));
    }

    let mut meta_map = Map::new();
    if let Some(id) = node.attribute("id") {
        meta_map.insert("id".to_string(), Value::String(id.to_string()));
    }

    if let Some(val) = node.attribute("value") {
        let mut extensions = Vec::new();
        for child in node.children().filter(|c| c.is_element()) {
            if child.tag_name().name() == "extension" {
                let (ext_val, _ext_meta) = xml_element_to_value(source, &child)?;
                extensions.push(ext_val);
            }
        }
        if !extensions.is_empty() {
            meta_map.insert("extension".to_string(), Value::Array(extensions));
        }
        let prim = parse_primitive(val);
        let meta = if meta_map.is_empty() {
            None
        } else {
            Some(Value::Object(meta_map))
        };
        return Ok((prim, meta));
    }

    let mut obj = Map::new();
    if let Some(id) = node.attribute("id") {
        obj.insert("id".to_string(), Value::String(id.to_string()));
    }

    for child in node.children().filter(|c| c.is_element()) {
        process_xml_child(source, &mut obj, &child)?;
    }

    Ok((Value::Object(obj), None))
}

fn insert_json_property(
    map: &mut Map<String, Value>,
    name: &str,
    value: Value,
    meta: Option<Value>,
) {
    let entry = map.entry(name.to_string());
    match entry {
        serde_json::map::Entry::Vacant(v) => {
            v.insert(value);
        }
        serde_json::map::Entry::Occupied(mut o) => match o.get_mut() {
            Value::Array(arr) => arr.push(value),
            existing => {
                let old = existing.take();
                *existing = Value::Array(vec![old, value]);
            }
        },
    }

    if meta.is_none() && !map.contains_key(&format!("_{}", name)) {
        return;
    }

    let meta_key = format!("_{}", name);
    let value_is_array = matches!(map.get(name), Some(Value::Array(_)));
    let value_count = match map.get(name) {
        Some(Value::Array(arr)) => arr.len(),
        Some(_) => 1,
        None => 0,
    };

    match map.entry(meta_key) {
        serde_json::map::Entry::Vacant(v) => {
            if let Some(m) = meta {
                if value_is_array {
                    let mut arr = Vec::new();
                    if value_count > 1 {
                        arr.resize(value_count - 1, Value::Null);
                    }
                    arr.push(m);
                    v.insert(Value::Array(arr));
                } else {
                    v.insert(m);
                }
            }
        }
        serde_json::map::Entry::Occupied(mut o) => match o.get_mut() {
            Value::Array(arr) => {
                if let Some(m) = meta {
                    if arr.len() + 1 < value_count {
                        arr.resize(value_count - 1, Value::Null);
                    }
                    arr.push(m);
                } else {
                    arr.push(Value::Null);
                }
            }
            existing => {
                if value_is_array {
                    let first = existing.take();
                    let mut arr = Vec::new();
                    arr.push(first);
                    if value_count > 1 {
                        arr.resize(value_count - 1, Value::Null);
                    }
                    if let Some(m) = meta {
                        arr.push(m);
                    } else {
                        arr.push(Value::Null);
                    }
                    *existing = Value::Array(arr);
                } else if let Some(m) = meta {
                    *existing = m;
                }
            }
        },
    }
}

fn parse_primitive(input: &str) -> Value {
    match input {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => {
            if let Ok(int) = input.parse::<i64>() {
                Value::Number(int.into())
            } else {
                Value::String(input.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_to_xml_basic_patient() {
        let json = r#"
        {
            "resourceType": "Patient",
            "id": "pat-1",
            "active": true,
            "name": [
                { "family": "Everyman", "given": ["Adam"] }
            ]
        }
        "#;

        let xml = json_to_xml(json).expect("conversion failed");
        assert!(xml.contains("<Patient"));
        assert!(xml.contains(r#"<id value="pat-1"/>"#));
        assert!(xml.contains(r#"<active value="true"/>"#));
        assert!(xml.contains(r#"<family value="Everyman"/>"#));
    }

    #[test]
    fn xml_to_json_round_trip() {
        let xml = r#"
        <Patient xmlns="http://hl7.org/fhir">
            <id value="p1"/>
            <active value="true"/>
            <name>
                <family value="Everyman"/>
                <given value="Adam"/>
            </name>
        </Patient>
        "#;

        let json = xml_to_json(xml).expect("xml->json failed");
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["resourceType"], "Patient");
        assert_eq!(value["id"], "p1");
        assert_eq!(value["active"], true);
        let family = if value["name"].is_array() {
            value["name"][0]["family"].clone()
        } else {
            value["name"]["family"].clone()
        };
        assert_eq!(family, "Everyman");
    }

    #[test]
    fn primitive_metadata_survives_roundtrip() {
        let json = r#"
        {
            "resourceType": "Patient",
            "birthDate": "1974-12-25",
            "_birthDate": { "id": "bd1" }
        }
        "#;

        let xml = json_to_xml(json).unwrap();
        assert!(xml.contains("<birthDate"));
        assert!(xml.contains(r#"value="1974-12-25""#));
        assert!(xml.contains(r#"id="bd1""#));

        let back = xml_to_json(&xml).unwrap();
        let val: Value = serde_json::from_str(&back).unwrap();
        assert_eq!(val["birthDate"], "1974-12-25");
        assert_eq!(val["_birthDate"]["id"], "bd1");
    }
}
