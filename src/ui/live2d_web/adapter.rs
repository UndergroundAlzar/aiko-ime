use std::error::Error;
use std::fmt;

use super::bridge::{BridgeError, CommandReceipt, HostBridge};
use super::protocol::{HostCommand, RuntimeEventEnvelope};

/// Minimal surface needed from a concrete WebView2 controller.
///
/// A future window integration can implement this trait with
/// `ICoreWebView2::ExecuteScript` or a WebView2 crate without changing the
/// command/event protocol.
pub trait ScriptExecutor {
    type Error: Error + Send + Sync + 'static;

    fn execute_script(&mut self, script: &str) -> Result<(), Self::Error>;
}

#[derive(Debug)]
pub enum AdapterError<E> {
    Bridge(BridgeError),
    Execute(E),
}

impl<E: fmt::Display> fmt::Display for AdapterError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bridge(error) => error.fmt(formatter),
            Self::Execute(error) => write!(formatter, "WebView2 ExecuteScript failed: {error}"),
        }
    }
}

impl<E: Error + 'static> Error for AdapterError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Bridge(error) => Some(error),
            Self::Execute(error) => Some(error),
        }
    }
}

/// Protocol adapter only; it does not create a WebView2 environment or window.
pub struct WebView2HostAdapter<E> {
    bridge: HostBridge,
    executor: E,
}

impl<E: ScriptExecutor> WebView2HostAdapter<E> {
    pub fn new(executor: E) -> Self {
        Self {
            bridge: HostBridge::new(),
            executor,
        }
    }

    pub fn send(&mut self, command: HostCommand) -> Result<CommandReceipt, AdapterError<E::Error>> {
        let (receipt, json) = self
            .bridge
            .encode_command(command)
            .map_err(AdapterError::Bridge)?;
        let script = format!("window.AikoImeLive2D.dispatch({json});");
        self.executor
            .execute_script(&script)
            .map_err(AdapterError::Execute)?;
        Ok(receipt)
    }

    pub fn on_web_message(
        &self,
        json: &str,
    ) -> Result<RuntimeEventEnvelope, AdapterError<E::Error>> {
        self.bridge.decode_event(json).map_err(AdapterError::Bridge)
    }

    pub fn into_executor(self) -> E {
        self.executor
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use super::{ScriptExecutor, WebView2HostAdapter};
    use crate::ui::live2d_web::{HostCommand, MotionPriority};

    #[derive(Default)]
    struct RecordingExecutor {
        scripts: Vec<String>,
    }

    impl ScriptExecutor for RecordingExecutor {
        type Error = Infallible;

        fn execute_script(&mut self, script: &str) -> Result<(), Self::Error> {
            self.scripts.push(script.to_string());
            Ok(())
        }
    }

    #[test]
    fn adapter_dispatches_json_without_string_interpolation() {
        let mut adapter = WebView2HostAdapter::new(RecordingExecutor::default());
        adapter
            .send(HostCommand::PlayMotion {
                group: "TapBody".to_string(),
                index: 1,
                priority: MotionPriority::Normal,
            })
            .unwrap();

        let executor = adapter.into_executor();
        assert_eq!(executor.scripts.len(), 1);
        assert!(executor.scripts[0].starts_with("window.AikoImeLive2D.dispatch({"));
        assert!(executor.scripts[0].contains(r#""group":"TapBody""#));
        assert!(executor.scripts[0].ends_with(");"));
    }
}
