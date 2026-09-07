use crate::{Diagnostic, Limits};
use rhai::{Array, Dynamic, ImmutableString, Map, FLOAT, INT};
use serde_json::{Number, Value};
use std::io::{self, Write};

struct Budget<'a> {
    limits: &'a Limits,
    kind: &'static str,
    max_bytes: usize,
    nodes: usize,
    raw_bytes: usize,
}

impl Budget<'_> {
    fn fail(&self, description: &str) -> Diagnostic {
        Diagnostic::new(self.kind, description)
    }

    fn bytes(&mut self, count: usize) -> Result<(), Diagnostic> {
        self.raw_bytes = self
            .raw_bytes
            .checked_add(count)
            .filter(|total| *total <= self.max_bytes)
            .ok_or_else(|| self.fail("JSON exceeds byte limit"))?;
        Ok(())
    }

    fn node(&mut self, depth: usize) -> Result<(), Diagnostic> {
        if depth > self.limits.max_json_depth {
            return Err(self.fail("JSON exceeds nesting depth limit"));
        }
        self.nodes += 1;
        if self.nodes > self.limits.max_json_nodes {
            return Err(self.fail("JSON exceeds value count limit"));
        }
        // A lower bound on serialized size, charged before allocating output.
        self.bytes(1)
    }

    fn string(&mut self, value: &str) -> Result<(), Diagnostic> {
        if value.len() > self.limits.max_string_bytes {
            return Err(self.fail("JSON string or key exceeds byte limit"));
        }
        self.bytes(value.len())
    }

    fn input(&mut self, value: &Value, depth: usize) -> Result<(), Diagnostic> {
        self.node(depth)?;
        match value {
            Value::String(value) => self.string(value)?,
            Value::Array(values) => {
                if values.len() > self.limits.max_array_len {
                    return Err(self.fail("JSON array exceeds entry limit"));
                }
                for value in values {
                    self.input(value, depth + 1)?;
                }
            }
            Value::Object(values) => {
                if values.len() > self.limits.max_map_len {
                    return Err(self.fail("JSON object exceeds entry limit"));
                }
                for (key, value) in values {
                    self.string(key)?;
                    self.input(value, depth + 1)?;
                }
            }
            Value::Number(number) => {
                if number.is_u64() && number.as_i64().is_none() {
                    return Err(Diagnostic::new(
                        "input",
                        "JSON integer is outside signed 64-bit range",
                    ));
                }
                if !number.as_f64().is_some_and(f64::is_finite) {
                    return Err(Diagnostic::new(
                        "input",
                        "JSON number is not finite binary64",
                    ));
                }
            }
            Value::Null | Value::Bool(_) => {}
        }
        Ok(())
    }

    fn output(&mut self, value: &Dynamic, depth: usize) -> Result<Value, Diagnostic> {
        self.node(depth)?;
        // Inputs are copied JSON; closures and dynamic function calls are
        // disabled, so scripts cannot manufacture shared Dynamic values.
        // Tanu's existing no_closure feature also removes is_shared().
        if value.is_unit() {
            return Ok(Value::Null);
        }
        if value.is::<bool>() {
            return Ok(Value::Bool(
                value.as_bool().map_err(|e| Diagnostic::new("output", e))?,
            ));
        }
        if value.is::<INT>() {
            let number = value.as_int().map_err(|e| Diagnostic::new("output", e))?;
            return Ok(Value::Number(number.into()));
        }
        if value.is::<FLOAT>() {
            let number = value.as_float().map_err(|e| Diagnostic::new("output", e))?;
            let number = Number::from_f64(number)
                .ok_or_else(|| Diagnostic::new("output", "script returned a non-finite number"))?;
            return Ok(Value::Number(number));
        }
        if value.is::<ImmutableString>() {
            let text = value
                .read_lock::<ImmutableString>()
                .ok_or_else(|| Diagnostic::new("output", "cannot read output string"))?;
            self.string(&text)?;
            return Ok(Value::String(text.to_string()));
        }
        if value.is::<Array>() {
            let values = value
                .read_lock::<Array>()
                .ok_or_else(|| Diagnostic::new("output", "cannot read output array"))?;
            if values.len() > self.limits.max_array_len {
                return Err(self.fail("JSON array exceeds entry limit"));
            }
            let mut result = Vec::new();
            for item in values.iter() {
                result.push(self.output(item, depth + 1)?);
            }
            return Ok(Value::Array(result));
        }
        if value.is::<Map>() {
            let values = value
                .read_lock::<Map>()
                .ok_or_else(|| Diagnostic::new("output", "cannot read output object"))?;
            if values.len() > self.limits.max_map_len {
                return Err(self.fail("JSON object exceeds entry limit"));
            }
            let mut result = serde_json::Map::new();
            for (key, item) in values.iter() {
                self.string(key)?;
                let item = self.output(item, depth + 1)?;
                result.insert(key.to_string(), item);
            }
            return Ok(Value::Object(result));
        }
        Err(Diagnostic::new("output", "script returned a non-JSON type"))
    }
}

struct ByteCounter {
    bytes: usize,
    maximum: usize,
}
impl Write for ByteCounter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(buffer.len())
            .filter(|total| *total <= self.maximum)
            .ok_or_else(|| io::Error::other("JSON byte limit exceeded"))?;
        Ok(buffer.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn check_encoded_size(value: &Value, maximum: usize, kind: &str) -> Result<(), Diagnostic> {
    // Count actual JSON escapes/punctuation without allocating a serialized copy.
    serde_json::to_writer(&mut ByteCounter { bytes: 0, maximum }, value)
        .map_err(|_| Diagnostic::new(kind, "serialized JSON exceeds byte limit"))
}

pub(super) fn validate_input(input: &Value, limits: &Limits) -> Result<(), Diagnostic> {
    Budget {
        limits,
        kind: "inputLimit",
        max_bytes: limits.max_input_bytes,
        nodes: 0,
        raw_bytes: 0,
    }
    .input(input, 0)?;
    check_encoded_size(input, limits.max_input_bytes, "inputLimit")
}

pub(super) fn output_json(output: &Dynamic, limits: &Limits) -> Result<Value, Diagnostic> {
    let result = Budget {
        limits,
        kind: "outputLimit",
        max_bytes: limits.max_output_bytes,
        nodes: 0,
        raw_bytes: 0,
    }
    .output(output, 0)?;
    check_encoded_size(&result, limits.max_output_bytes, "outputLimit")?;
    Ok(result)
}
