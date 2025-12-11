//! OSC-IR message representation used by transports and runtime.

use std::fmt::Write;

use kitu_core::Result;

/// Supported OSC-IR argument types.
#[derive(Debug, Clone, PartialEq)]
pub enum OscArg {
    Int(i32),
    Float(f32),
    Str(String),
    Bool(bool),
}

/// OSC-IR message consisting of an address and a list of arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct OscMessage {
    /// OSC address pattern (e.g. `/player/move`).
    pub address: String,
    /// Arguments associated with the message, ordered as sent on the wire.
    pub args: Vec<OscArg>,
}

impl OscMessage {
    /// Creates a new message with the provided address.
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            args: Vec::new(),
        }
    }

    /// Appends an argument to the message.
    pub fn push_arg(&mut self, arg: OscArg) {
        self.args.push(arg);
    }

    /// Renders the message into a debug-friendly string for logging.
    pub fn to_debug_string(&self) -> Result<String> {
        let mut buf = String::new();
        write!(&mut buf, "{}(", self.address).unwrap();
        for (i, arg) in self.args.iter().enumerate() {
            if i > 0 {
                buf.push_str(", ");
            }
            match arg {
                OscArg::Int(v) => write!(&mut buf, "{v}").unwrap(),
                OscArg::Float(v) => write!(&mut buf, "{v}").unwrap(),
                OscArg::Str(v) => write!(&mut buf, "\"{v}\"").unwrap(),
                OscArg::Bool(v) => write!(&mut buf, "{v}").unwrap(),
            }
        }
        buf.push(')');
        Ok(buf)
    }
}

/// A collection of messages bundled for atomic delivery.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct OscBundle {
    /// Messages included in the bundle in send order.
    pub messages: Vec<OscMessage>,
}

impl OscBundle {
    /// Creates an empty bundle.
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    /// Pushes a message into the bundle.
    pub fn push(&mut self, message: OscMessage) {
        self.messages.push(message);
    }

    /// Returns the number of contained messages.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Whether the bundle contains no messages.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_building_and_debugging() {
        let mut msg = OscMessage::new("/player/move");
        msg.push_arg(OscArg::Int(1));
        msg.push_arg(OscArg::Bool(true));
        let rendered = msg.to_debug_string().unwrap();
        assert!(rendered.contains("/player/move"));
        assert!(rendered.contains("true"));
        assert_eq!(msg.args.len(), 2);
    }

    #[test]
    fn bundle_collects_messages() {
        let mut bundle = OscBundle::new();
        assert!(bundle.is_empty());
        bundle.push(OscMessage::new("/a"));
        bundle.push(OscMessage::new("/b"));
        assert_eq!(bundle.len(), 2);
    }
}
