use std::sync::Arc;

use native_windows_gui::NoticeSender;
use parking_lot::Mutex;
use pocket_relay_client_shared::events::TunnelEvent;

/// State of the tunnel, basically the same as [TunnelEvent] just with
/// an additional state for the initially not connected tunnel
#[derive(Default, Clone, Copy)]
pub enum TunnelState {
    /// Not connected
    #[default]
    NotConnected,

    /// Tunnel is not currently available based on the server / configuration
    Unavailable,

    /// Creating the tunnel server
    Creating,

    /// Performing a handshake to try and connect
    /// (UDP tunnel only, the HTTP tunnel does not have a handshake to track it goes from creating -> connected)
    Handshaking,

    /// Currently connected
    Connected,

    /// Attempting to reconnect
    RetryConnect,

    /// Error occurred while connecting and unable to connect, reached limit
    /// of connection retries
    Error,
}

impl From<TunnelEvent> for TunnelState {
    fn from(value: TunnelEvent) -> Self {
        match value {
            TunnelEvent::Creating => TunnelState::Creating,
            TunnelEvent::Handshaking => TunnelState::Handshaking,
            TunnelEvent::Connected => TunnelState::Connected,
            TunnelEvent::RetryConnect => TunnelState::RetryConnect,
            TunnelEvent::Error => TunnelState::Error,
        }
    }
}

pub struct TunnelStateRef {
    /// The shared tunnel state to update
    tunnel_state: Arc<Mutex<TunnelState>>,

    /// Notice sender which is lazily initialized when the UI becomes
    /// available
    notice_sender: NoticeSender,
}

impl TunnelStateRef {
    pub fn new(tunnel_state: Arc<Mutex<TunnelState>>, notice_sender: NoticeSender) -> Self {
        Self {
            tunnel_state,
            notice_sender,
        }
    }

    pub fn set_state(&self, state: TunnelState) {
        {
            *self.tunnel_state.lock() = state;
        }

        self.notice_sender.notice();
    }
}
