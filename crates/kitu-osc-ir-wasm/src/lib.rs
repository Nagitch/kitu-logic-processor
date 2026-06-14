//! WASM bindings for browser-side OSC-IR message construction.

use kitu_osc_ir::{OscArg, OscMessage};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JsonOscMessage {
    pub address: String,
    #[serde(default)]
    pub args: Vec<JsonOscArg>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
pub enum JsonOscArg {
    Int(i32),
    Int64(i64),
    Float(f32),
    Str(String),
    Bool(bool),
}

impl From<&OscMessage> for JsonOscMessage {
    fn from(message: &OscMessage) -> Self {
        Self {
            address: message.address.clone(),
            args: message.args.iter().map(JsonOscArg::from).collect(),
        }
    }
}

impl From<&OscArg> for JsonOscArg {
    fn from(value: &OscArg) -> Self {
        match value {
            OscArg::Int(value) => JsonOscArg::Int(*value),
            OscArg::Int64(value) => JsonOscArg::Int64(*value),
            OscArg::Float(value) => JsonOscArg::Float(*value),
            OscArg::Str(value) => JsonOscArg::Str(value.clone()),
            OscArg::Bool(value) => JsonOscArg::Bool(*value),
        }
    }
}

impl From<JsonOscArg> for OscArg {
    fn from(value: JsonOscArg) -> Self {
        match value {
            JsonOscArg::Int(value) => OscArg::Int(value),
            JsonOscArg::Int64(value) => OscArg::Int64(value),
            JsonOscArg::Float(value) => OscArg::Float(value),
            JsonOscArg::Str(value) => OscArg::Str(value),
            JsonOscArg::Bool(value) => OscArg::Bool(value),
        }
    }
}

impl From<JsonOscMessage> for OscMessage {
    fn from(value: JsonOscMessage) -> Self {
        let mut message = OscMessage::new(value.address);
        for arg in value.args {
            message.push_arg(arg.into());
        }
        message
    }
}

#[wasm_bindgen]
pub struct OscMessageBuilder {
    inner: OscMessage,
}

#[wasm_bindgen]
impl OscMessageBuilder {
    #[wasm_bindgen(constructor)]
    pub fn new(address: String) -> Self {
        Self {
            inner: OscMessage::new(address),
        }
    }

    pub fn push_int(&mut self, value: i32) {
        self.inner.push_arg(OscArg::Int(value));
    }

    pub fn push_int64(&mut self, value: i64) {
        self.inner.push_arg(OscArg::Int64(value));
    }

    pub fn push_float(&mut self, value: f32) {
        self.inner.push_arg(OscArg::Float(value));
    }

    pub fn push_str(&mut self, value: String) {
        self.inner.push_arg(OscArg::Str(value));
    }

    pub fn push_bool(&mut self, value: bool) {
        self.inner.push_arg(OscArg::Bool(value));
    }

    pub fn build_json(&self) -> Result<JsValue, JsValue> {
        to_js_message(&self.inner)
    }
}

#[wasm_bindgen]
pub fn admin_world_spawn(kind: String, x: f32, y: f32, z: f32) -> Result<JsValue, JsValue> {
    let mut message = OscMessage::new("/admin/world/spawn");
    message.push_arg(OscArg::Str(kind));
    message.push_arg(OscArg::Float(x));
    message.push_arg(OscArg::Float(y));
    message.push_arg(OscArg::Float(z));
    to_js_message(&message)
}

#[wasm_bindgen]
pub fn admin_world_move(id: String, x: f32, y: f32, z: f32) -> Result<JsValue, JsValue> {
    let mut message = OscMessage::new("/admin/world/move");
    message.push_arg(OscArg::Str(id));
    message.push_arg(OscArg::Float(x));
    message.push_arg(OscArg::Float(y));
    message.push_arg(OscArg::Float(z));
    to_js_message(&message)
}

#[wasm_bindgen]
pub fn admin_world_reset() -> Result<JsValue, JsValue> {
    to_js_message(&OscMessage::new("/admin/world/reset"))
}

fn to_js_message(message: &OscMessage) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(&JsonOscMessage::from(message))
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_message_round_trips_into_osc_ir() {
        let json = JsonOscMessage {
            address: "/admin/world/spawn".to_string(),
            args: vec![
                JsonOscArg::Str("enemy".to_string()),
                JsonOscArg::Float(1.0),
                JsonOscArg::Float(2.0),
                JsonOscArg::Float(3.0),
            ],
        };

        let osc = OscMessage::from(json.clone());
        assert_eq!(osc.address, "/admin/world/spawn");
        assert_eq!(osc.args[0], OscArg::Str("enemy".to_string()));
        assert_eq!(JsonOscMessage::from(&osc), json);
    }

    #[test]
    fn admin_builders_create_backend_compatible_shapes() {
        let mut message = OscMessage::new("/admin/world/move");
        message.push_arg(OscArg::Str("obj-1".to_string()));
        message.push_arg(OscArg::Float(4.0));
        message.push_arg(OscArg::Float(5.0));
        message.push_arg(OscArg::Float(6.0));

        let json = JsonOscMessage::from(&message);
        let serialized = serde_json::to_string(&json).unwrap();
        let decoded: JsonOscMessage = serde_json::from_str(&serialized).unwrap();
        let osc = OscMessage::from(decoded);

        assert_eq!(osc, message);
    }
}
