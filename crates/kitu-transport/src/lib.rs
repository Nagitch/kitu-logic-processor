//! Transport abstraction for delivering OSC-IR messages between peers.
//!
//! # Responsibilities
//! - Define the [`Transport`] trait and event model for moving OSC/IR messages around the system.
//! - Host concrete adapters (e.g., in-memory channels) while staying open to networked transports.
//! - Keep delivery concerns isolated from gameplay logic and runtime scheduling.
//!
//! # Integration
//! Transports bridge OSC/IR types (`kitu-osc-ir`) with the runtime loop (`kitu-runtime`). See
//! `doc/crates-overview.md` for adapter expectations and how events flow into ECS systems.

use std::collections::VecDeque;

use kitu_core::{KituError, Result};
use kitu_osc_ir::{OscBundle, OscMessage};

/// Event emitted by a transport implementation.
///
/// Implementations should emit `Connected` and `Disconnected` when peer state
/// changes, and `Message` whenever an OSC bundle is ready for processing.
#[derive(Debug, Clone, PartialEq)]
pub enum TransportEvent {
    /// Transport is now connected to its peer.
    Connected,
    /// Transport has disconnected and should not be used until reinitialized.
    Disconnected,
    /// An OSC bundle is ready for processing.
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
///
/// This lightweight transport is intentionally synchronous and allocates minimal
/// resources, making it ideal for unit tests or deterministic playback.
#[derive(Default)]
pub struct LocalChannel {
    inbox: VecDeque<TransportEvent>,
}

impl LocalChannel {
    /// Creates a connected channel with no queued messages.
    ///
    /// # Examples
    ///
    /// ```
    /// use kitu_transport::{LocalChannel, Transport, TransportEvent};
    ///
    /// let mut channel = LocalChannel::connected();
    /// assert_eq!(channel.poll_event(), Some(TransportEvent::Connected));
    /// ```
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
///
/// The current placeholder always returns [`KituError::NotImplemented`], but the
/// helper documents the expected shape of a graceful shutdown.
///
/// # Examples
///
/// ```
/// use kitu_core::KituError;
/// use kitu_transport::{disconnect, LocalChannel};
///
/// let mut channel = LocalChannel::default();
/// let result = disconnect(&mut channel);
/// assert!(matches!(result, Err(KituError::NotImplemented(_))));
/// ```
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
