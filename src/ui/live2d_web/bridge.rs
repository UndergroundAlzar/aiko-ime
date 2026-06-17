use std::error::Error;
use std::fmt;

use super::protocol::{HostCommand, HostCommandEnvelope, RuntimeEventEnvelope, PROTOCOL_VERSION};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandReceipt {
    pub request_id: u64,
}

#[derive(Debug)]
pub enum BridgeError {
    InvalidCommand(&'static str),
    InvalidJson(serde_json::Error),
    ProtocolVersion { expected: u16, received: u16 },
}

impl fmt::Display for BridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCommand(field) => {
                write!(formatter, "invalid Live2D command field: {field}")
            }
            Self::InvalidJson(error) => write!(formatter, "invalid Live2D bridge JSON: {error}"),
            Self::ProtocolVersion { expected, received } => write!(
                formatter,
                "unsupported Live2D bridge protocol version {received}; expected {expected}"
            ),
        }
    }
}

impl Error for BridgeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidJson(error) => Some(error),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for BridgeError {
    fn from(value: serde_json::Error) -> Self {
        Self::InvalidJson(value)
    }
}

#[derive(Debug, Default)]
pub struct HostBridge {
    next_request_id: u64,
}

impl HostBridge {
    pub fn new() -> Self {
        Self { next_request_id: 1 }
    }

    pub fn encode_command(
        &mut self,
        command: HostCommand,
    ) -> Result<(CommandReceipt, String), BridgeError> {
        command.validate().map_err(BridgeError::InvalidCommand)?;

        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        let envelope = HostCommandEnvelope {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            payload: command,
        };
        let json = serde_json::to_string(&envelope)?;
        Ok((CommandReceipt { request_id }, json))
    }

    pub fn decode_event(&self, json: &str) -> Result<RuntimeEventEnvelope, BridgeError> {
        let event: RuntimeEventEnvelope = serde_json::from_str(json)?;
        if event.protocol_version != PROTOCOL_VERSION {
            return Err(BridgeError::ProtocolVersion {
                expected: PROTOCOL_VERSION,
                received: event.protocol_version,
            });
        }
        Ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::{BridgeError, HostBridge};
    use crate::ui::live2d_web::{
        BuiltinModel, HostCommand, MotionPriority, RuntimeEvent, PROTOCOL_VERSION,
    };

    #[test]
    fn command_envelopes_are_stable_and_sequenced() {
        let mut bridge = HostBridge::new();
        let (first, json) = bridge
            .encode_command(HostCommand::LoadBuiltinModel {
                model: BuiltinModel::Aiko,
            })
            .unwrap();
        let (second, _) = bridge
            .encode_command(HostCommand::PlayMotion {
                group: "Idle".to_string(),
                index: 0,
                priority: MotionPriority::Idle,
            })
            .unwrap();

        assert_eq!(first.request_id, 1);
        assert_eq!(second.request_id, 2);
        assert!(json.contains(r#""type":"load_builtin_model""#));
        assert!(json.contains(r#""model":"aiko""#));
    }

    #[test]
    fn invalid_numeric_commands_are_rejected_before_webview_dispatch() {
        let error = HostBridge::new()
            .encode_command(HostCommand::SetLipSync { value: 1.5 })
            .unwrap_err();
        assert!(matches!(
            error,
            BridgeError::InvalidCommand("lip sync value")
        ));
    }

    #[test]
    fn runtime_events_decode_with_request_correlation() {
        let json = format!(
            r#"{{
                "protocol_version": {PROTOCOL_VERSION},
                "request_id": 7,
                "payload": {{
                    "type": "hit_area",
                    "name": "Head",
                    "x": 0.25,
                    "y": -0.5
                }}
            }}"#
        );
        let event = HostBridge::new().decode_event(&json).unwrap();
        assert_eq!(event.request_id, Some(7));
        assert!(matches!(
            event.payload,
            RuntimeEvent::HitArea { ref name, .. } if name == "Head"
        ));
    }
}
