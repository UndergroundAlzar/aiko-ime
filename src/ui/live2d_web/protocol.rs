use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinModel {
    Aiko,
    AikoPajama,
    AikoRain,
    AikoWinter,
    AikoCelebration,
    AikoCagOperator,
}

impl BuiltinModel {
    pub const fn model3_relative_path(self) -> &'static str {
        match self {
            Self::Aiko => "assets/live2d/aiko/runtime/aiko.model3.json",
            Self::AikoPajama => "assets/live2d/aiko_pajama/runtime/aiko_pajama.model3.json",
            Self::AikoRain => "assets/live2d/aiko_rain/runtime/aiko_rain.model3.json",
            Self::AikoWinter => "assets/live2d/aiko_winter/runtime/aiko_winter.model3.json",
            Self::AikoCelebration => {
                "assets/live2d/aiko_celebration/runtime/aiko_celebration.model3.json"
            }
            Self::AikoCagOperator => "assets/live2d/aiko_cag/runtime/aiko_cag.model3.json",
        }
    }

    pub const fn all() -> [Self; 6] {
        [
            Self::Aiko,
            Self::AikoPajama,
            Self::AikoRain,
            Self::AikoWinter,
            Self::AikoCelebration,
            Self::AikoCagOperator,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionPriority {
    Idle,
    Normal,
    Force,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostCommand {
    LoadBuiltinModel {
        model: BuiltinModel,
    },
    SetParameter {
        id: String,
        value: f32,
        #[serde(default = "default_parameter_weight")]
        weight: f32,
    },
    PlayMotion {
        group: String,
        index: usize,
        priority: MotionPriority,
    },
    SetExpression {
        id: String,
    },
    ResetExpression,
    SetLookAt {
        x: f32,
        y: f32,
    },
    SetLipSync {
        value: f32,
    },
    HitTest {
        x: f32,
        y: f32,
    },
    StopAllMotions,
}

fn default_parameter_weight() -> f32 {
    1.0
}

impl HostCommand {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::LoadBuiltinModel { .. } | Self::ResetExpression | Self::StopAllMotions => Ok(()),
            Self::SetParameter { id, value, weight } => {
                require_name(id, "parameter id")?;
                require_finite(*value, "parameter value")?;
                require_range(*weight, 0.0, 1.0, "parameter weight")
            }
            Self::PlayMotion { group, .. } => require_name(group, "motion group"),
            Self::SetExpression { id } => require_name(id, "expression id"),
            Self::SetLookAt { x, y } | Self::HitTest { x, y } => {
                require_range(*x, -1.0, 1.0, "x")?;
                require_range(*y, -1.0, 1.0, "y")
            }
            Self::SetLipSync { value } => require_range(*value, 0.0, 1.0, "lip sync value"),
        }
    }
}

fn require_name(value: &str, label: &'static str) -> Result<(), &'static str> {
    if value.trim().is_empty() {
        Err(label)
    } else {
        Ok(())
    }
}

fn require_finite(value: f32, label: &'static str) -> Result<(), &'static str> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(label)
    }
}

fn require_range(
    value: f32,
    minimum: f32,
    maximum: f32,
    label: &'static str,
) -> Result<(), &'static str> {
    if value.is_finite() && (minimum..=maximum).contains(&value) {
        Ok(())
    } else {
        Err(label)
    }
}

#[cfg(test)]
mod tests {
    use super::BuiltinModel;

    #[test]
    fn every_builtin_model_has_a_fixed_model3_path() {
        let paths: Vec<_> = BuiltinModel::all()
            .into_iter()
            .map(BuiltinModel::model3_relative_path)
            .collect();
        assert_eq!(paths.len(), 6);
        assert!(paths.iter().all(|path| {
            path.starts_with("assets/live2d/")
                && path.ends_with(".model3.json")
                && !path.contains("..")
        }));
        let unique: std::collections::BTreeSet<_> = paths.iter().copied().collect();
        assert_eq!(unique.len(), paths.len());
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostCommandEnvelope {
    pub protocol_version: u16,
    pub request_id: u64,
    pub payload: HostCommand,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeEventEnvelope {
    pub protocol_version: u16,
    #[serde(default)]
    pub request_id: Option<u64>,
    pub payload: RuntimeEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeEvent {
    RuntimeReady {
        core_version: String,
        adapter_version: String,
    },
    ModelLoaded {
        model: BuiltinModel,
    },
    MotionFinished {
        group: String,
        index: usize,
    },
    HitArea {
        name: String,
        x: f32,
        y: f32,
    },
    Error {
        code: String,
        message: String,
        recoverable: bool,
    },
    Log {
        level: String,
        message: String,
    },
}
