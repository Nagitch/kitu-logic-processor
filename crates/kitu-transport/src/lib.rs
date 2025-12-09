//! Transport abstraction for delivering OSC-IR messages between peers.

use std::collections::VecDeque;

use kitu_core::{KituError, Result};
use kitu_osc_ir::{OscBundle, OscMessage};

/// Event emitted by a transport implementation.
#[derive(Debug, Clone, PartialEq)]
pub enum TransportEvent {
    Connected,
    Disconnected,
    Message(OscBundle),
}

/// Common interface for transports.
pub trait Transport {
    /// Sends a single OSC message.
    fn send(&mut self, message: OscMessage) -> Result<()>;

    /// Receives the next pending event, if any.
    fn poll_event(&mut self) -> Option<TransportEvent>;
}

/// In-memory channel transport useful for tests and local simulations.
#[derive(Default)]
pub struct LocalChannel {
    inbox: VecDeque<TransportEvent>,
}

impl LocalChannel {
    /// Creates a connected channel with no queued messages.
    pub fn connected() -> Self {
        let mut channel = Self::default();
        channel.inbox.push_back(TransportEvent::Connected);
        channel
    }
}

impl Transport for LocalChannel {
    fn send(&mut self, message: OscMessage) -> Result<()> {
        let mut bundle = OscBundle::new();
        bundle.push(message);
        self.inbox.push_back(TransportEvent::Message(bundle));
        Ok(())
    }

    fn poll_event(&mut self) -> Option<TransportEvent> {
        self.inbox.pop_front()
    }
}

/// Validates that transports transition to disconnected state.
pub fn disconnect<T: Transport>(transport: &mut T) -> Result<()> {
    let _ = transport;
    Err(KituError::NotImplemented("disconnect".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_channel_reports_connection_then_messages() {
        let mut channel = LocalChannel::connected();
        assert_eq!(channel.poll_event(), Some(TransportEvent::Connected));

        channel
            .send(OscMessage::new("/ping"))
            .expect("send should enqueue message");
        match channel.poll_event() {
            Some(TransportEvent::Message(bundle)) => {
                assert_eq!(bundle.len(), 1);
                assert_eq!(bundle.messages[0].address, "/ping");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn disconnect_returns_not_implemented_error() {
        let mut channel = LocalChannel::default();
        let result = disconnect(&mut channel);
        assert!(matches!(result, Err(KituError::NotImplemented(_))));
    }
}
