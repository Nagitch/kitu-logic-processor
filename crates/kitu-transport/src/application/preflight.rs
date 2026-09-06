//! Allocation-bounded structural scan before a format deserializer sees DTOs.
use super::{CodecError, CodecErrorKind, Encoding, Limits};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::collections::BTreeSet;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Map,
    Array,
    String,
    Float32,
    Other,
}
#[derive(Clone, Copy)]
struct Shape {
    kind: Kind,
    float_tag: bool,
}
impl Shape {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            float_tag: false,
        }
    }
}
#[derive(Clone, Copy)]
enum ArrayOf {
    Map,
    String,
}
fn array_field(key: &str) -> Option<ArrayOf> {
    match key {
        "features" => Some(ArrayOf::String),
        "bundles" | "messages" | "args" => Some(ArrayOf::Map),
        _ => None,
    }
}
fn element(array: ArrayOf, kind: Kind) -> Result<(), CodecError> {
    let expected = match array {
        ArrayOf::Map => Kind::Map,
        ArrayOf::String => Kind::String,
    };
    if kind != expected {
        return Err(malformed("array element has incorrect wire type"));
    }
    Ok(())
}
fn malformed(message: &str) -> CodecError {
    CodecError::new(CodecErrorKind::Malformed, message)
}
fn limit(message: &str) -> CodecError {
    CodecError::new(CodecErrorKind::Limit, message)
}
struct Budget {
    limits: Limits,
    nodes: usize,
}
impl Budget {
    fn enter(&mut self, depth: usize) -> Result<(), CodecError> {
        if depth > self.limits.max_depth {
            return Err(limit("structural depth limit exceeded"));
        }
        if self.nodes >= self.limits.max_nodes {
            return Err(limit("structural node limit exceeded"));
        }
        self.nodes += 1;
        Ok(())
    }
    fn collection(&self, length: usize) -> Result<(), CodecError> {
        if length > self.limits.max_collection_len {
            return Err(limit("collection length limit exceeded"));
        }
        Ok(())
    }
}
pub(super) fn check(bytes: &[u8], encoding: Encoding, limits: Limits) -> Result<(), CodecError> {
    if bytes.len() > limits.max_bytes {
        return Err(limit("frame byte limit exceeded"));
    }
    let mut budget = Budget { limits, nodes: 0 };
    let shape = match encoding {
        Encoding::Json => {
            let mut deserializer = serde_json::Deserializer::from_slice(bytes);
            let shape = JsonSeed {
                budget: &mut budget,
                depth: 1,
                array: None,
            }
            .deserialize(&mut deserializer)
            .map_err(|error| {
                let message = error.to_string();
                CodecError::new(
                    if message.contains("limit") {
                        CodecErrorKind::Limit
                    } else {
                        CodecErrorKind::Malformed
                    },
                    message,
                )
            })?;
            deserializer.end().map_err(|error| {
                CodecError::new(CodecErrorKind::TrailingData, error.to_string())
            })?;
            shape
        }
        Encoding::MessagePack => {
            let mut reader = MessagePack {
                bytes,
                position: 0,
                budget,
            };
            let shape = reader.value(1, None)?;
            if reader.position != bytes.len() {
                return Err(CodecError::new(
                    CodecErrorKind::TrailingData,
                    "trailing MessagePack data",
                ));
            }
            shape
        }
    };
    if shape.kind != Kind::Map {
        return Err(malformed("wire root must be a named map"));
    }
    Ok(())
}

struct JsonSeed<'a> {
    budget: &'a mut Budget,
    depth: usize,
    array: Option<ArrayOf>,
}
impl<'de> DeserializeSeed<'de> for JsonSeed<'_> {
    type Value = Shape;
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Shape, D::Error> {
        self.budget.enter(self.depth).map_err(de::Error::custom)?;
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for JsonSeed<'_> {
    type Value = Shape;
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("bounded application JSON")
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Shape, E> {
        Ok(Shape::new(Kind::Other))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Shape, E> {
        Ok(Shape::new(Kind::Other))
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Shape, E> {
        Ok(Shape::new(Kind::Other))
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Shape, E> {
        Ok(Shape::new(Kind::Other))
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Shape, E> {
        if !value.is_finite() {
            return Err(E::custom("nonfinite JSON number"));
        }
        Ok(Shape::new(Kind::Other))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Shape, E> {
        Ok(Shape {
            kind: Kind::String,
            float_tag: value == "float",
        })
    }
    fn visit_string<E: de::Error>(self, value: String) -> Result<Shape, E> {
        self.visit_str(&value)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Shape, A::Error> {
        let array = self
            .array
            .ok_or_else(|| de::Error::custom("wire structs must be named maps, not arrays"))?;
        if let Some(length) = sequence.size_hint() {
            self.budget.collection(length).map_err(de::Error::custom)?;
        }
        let mut length = 0usize;
        while let Some(shape) = sequence.next_element_seed(JsonSeed {
            budget: self.budget,
            depth: self.depth + 1,
            array: None,
        })? {
            length += 1;
            self.budget.collection(length).map_err(de::Error::custom)?;
            element(array, shape.kind).map_err(de::Error::custom)?;
        }
        Ok(Shape::new(Kind::Array))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Shape, A::Error> {
        let mut keys = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            self.budget
                .enter(self.depth + 1)
                .map_err(de::Error::custom)?;
            self.budget
                .collection(keys.len() + 1)
                .map_err(de::Error::custom)?;
            let array = array_field(&key);
            if !keys.insert(key) {
                return Err(de::Error::custom("duplicate map field"));
            }
            map.next_value_seed(JsonSeed {
                budget: self.budget,
                depth: self.depth + 1,
                array,
            })?;
        }
        Ok(Shape::new(Kind::Map))
    }
}

struct MessagePack<'a> {
    bytes: &'a [u8],
    position: usize,
    budget: Budget,
}
impl<'a> MessagePack<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], CodecError> {
        let end = self
            .position
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| malformed("truncated MessagePack value"))?;
        let bytes = &self.bytes[self.position..end];
        self.position = end;
        Ok(bytes)
    }
    fn marker(&mut self) -> Result<u8, CodecError> {
        Ok(self.take(1)?[0])
    }
    fn unsigned(&mut self, length: usize) -> Result<u64, CodecError> {
        Ok(self
            .take(length)?
            .iter()
            .fold(0, |n, byte| (n << 8) | u64::from(*byte)))
    }
    fn length(&mut self, bytes: usize) -> Result<usize, CodecError> {
        usize::try_from(self.unsigned(bytes)?).map_err(|_| malformed("MessagePack length overflow"))
    }
    fn string(&mut self, marker: u8) -> Result<&'a str, CodecError> {
        let length = match marker {
            0xa0..=0xbf => usize::from(marker & 31),
            0xd9 => self.length(1)?,
            0xda => self.length(2)?,
            0xdb => self.length(4)?,
            _ => return Err(malformed("map key must be a UTF-8 string")),
        };
        std::str::from_utf8(self.take(length)?).map_err(|_| malformed("invalid MessagePack UTF-8"))
    }
    fn value(&mut self, depth: usize, array: Option<ArrayOf>) -> Result<Shape, CodecError> {
        self.budget.enter(depth)?;
        let marker = self.marker()?;
        match marker {
            0x00..=0x7f | 0xe0..=0xff | 0xc0 | 0xc2 | 0xc3 => Ok(Shape::new(Kind::Other)),
            0xcc | 0xd0 => {
                self.take(1)?;
                Ok(Shape::new(Kind::Other))
            }
            0xcd | 0xd1 => {
                self.take(2)?;
                Ok(Shape::new(Kind::Other))
            }
            0xce | 0xd2 => {
                self.take(4)?;
                Ok(Shape::new(Kind::Other))
            }
            0xcf | 0xd3 => {
                self.take(8)?;
                Ok(Shape::new(Kind::Other))
            }
            0xca => {
                let bits = self.unsigned(4)? as u32;
                if !f32::from_bits(bits).is_finite() {
                    return Err(malformed("nonfinite MessagePack float32"));
                }
                Ok(Shape::new(Kind::Float32))
            }
            0xcb => Err(malformed("float64 is not part of application wire 1")),
            0xa0..=0xbf | 0xd9..=0xdb => {
                let value = self.string(marker)?;
                Ok(Shape {
                    kind: Kind::String,
                    float_tag: value == "float",
                })
            }
            0x90..=0x9f | 0xdc | 0xdd => {
                let length = match marker {
                    0xdc => self.length(2)?,
                    0xdd => self.length(4)?,
                    _ => usize::from(marker & 15),
                };
                self.budget.collection(length)?;
                let array = array
                    .ok_or_else(|| malformed("wire structs must be named maps, not arrays"))?;
                if length > self.bytes.len() - self.position
                    || length > self.budget.limits.max_nodes - self.budget.nodes
                {
                    return Err(limit(
                        "array declared length exceeds remaining byte/node limit",
                    ));
                }
                for _ in 0..length {
                    element(array, self.value(depth + 1, None)?.kind)?;
                }
                Ok(Shape::new(Kind::Array))
            }
            0x80..=0x8f | 0xde | 0xdf => {
                let length = match marker {
                    0xde => self.length(2)?,
                    0xdf => self.length(4)?,
                    _ => usize::from(marker & 15),
                };
                self.budget.collection(length)?;
                if length > (self.bytes.len() - self.position) / 2
                    || length > (self.budget.limits.max_nodes - self.budget.nodes) / 2
                {
                    return Err(limit(
                        "map declared length exceeds remaining byte/node limit",
                    ));
                }
                let mut keys = BTreeSet::new();
                let mut float_tag = false;
                let mut value_kind = None;
                for _ in 0..length {
                    self.budget.enter(depth + 1)?;
                    let marker = self.marker()?;
                    let key = self.string(marker)?;
                    if !keys.insert(key) {
                        return Err(malformed("duplicate map field"));
                    }
                    let value = self.value(depth + 1, array_field(key))?;
                    if key == "type" {
                        float_tag = value.float_tag;
                    }
                    if key == "value" {
                        value_kind = Some(value.kind);
                    }
                }
                if float_tag && value_kind != Some(Kind::Float32) {
                    return Err(malformed(
                        "typed float requires a float32 MessagePack marker",
                    ));
                }
                Ok(Shape::new(Kind::Map))
            }
            _ => Err(malformed(
                "unsupported MessagePack binary, extension, or reserved marker",
            )),
        }
    }
}
