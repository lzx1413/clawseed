//! Voice duplex event handling stub.
//!
//! Gated behind the `gateway-voice-duplex` feature flag.

/// Try to parse a voice event from a WebSocket message.
/// Returns None if the message is not a voice event.
pub fn try_parse_voice_event(_msg: &serde_json::Value) -> Option<VoiceEvent> {
    None
}

/// Handle a voice event. Returns an optional error frame to send back.
pub fn handle_voice_event(_event: VoiceEvent) -> Option<serde_json::Value> {
    None
}

/// A parsed voice duplex event.
#[derive(Debug)]
pub struct VoiceEvent {
    pub event_type: String,
}
