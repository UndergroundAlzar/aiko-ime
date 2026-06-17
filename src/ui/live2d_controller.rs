//! Renderer-independent Live2D state and interaction mapping.

use std::fmt;
use std::str::FromStr;

pub use super::desktop_pet::PetState;

const PARAM_LISTENING: &str = "ParamAikoMicGlow";
const PARAM_CAG_LISTENING: &str = "ParamAikoListening";
const PARAM_THINKING: &str = "ParamAikoThinking";
const PARAM_PET: &str = "ParamAikoPet";
const PARAM_SLEEP: &str = "ParamAikoSleep";
const PARAM_TEAR: &str = "ParamAikoTear";

/// The only model skins accepted by the bundled Live2D runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Live2dSkin {
    #[default]
    Default,
    Pajama,
    Rain,
    Winter,
    Celebration,
    CagOperator,
}

impl Live2dSkin {
    pub const ALL: [Self; 6] = [
        Self::Default,
        Self::Pajama,
        Self::Rain,
        Self::Winter,
        Self::Celebration,
        Self::CagOperator,
    ];

    pub const fn config_value(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Pajama => "pajama",
            Self::Rain => "rain",
            Self::Winter => "winter",
            Self::Celebration => "celebration",
            Self::CagOperator => "cag_operator",
        }
    }

    pub const fn model_id(self) -> &'static str {
        match self {
            Self::Default => "aiko",
            Self::Pajama => "aiko_pajama",
            Self::Rain => "aiko_rain",
            Self::Winter => "aiko_winter",
            Self::Celebration => "aiko_celebration",
            Self::CagOperator => "aiko_cag_operator",
        }
    }

    pub const fn model3_relative_path(self) -> &'static str {
        match self {
            Self::Default => "assets/live2d/aiko/runtime/aiko.model3.json",
            Self::Pajama => "assets/live2d/aiko_pajama/runtime/aiko_pajama.model3.json",
            Self::Rain => "assets/live2d/aiko_rain/runtime/aiko_rain.model3.json",
            Self::Winter => "assets/live2d/aiko_winter/runtime/aiko_winter.model3.json",
            Self::Celebration => {
                "assets/live2d/aiko_celebration/runtime/aiko_celebration.model3.json"
            }
            Self::CagOperator => "assets/live2d/aiko_cag/runtime/aiko_cag.model3.json",
        }
    }

    const fn listening_parameter(self) -> &'static str {
        match self {
            Self::CagOperator => PARAM_CAG_LISTENING,
            _ => PARAM_LISTENING,
        }
    }
}

impl fmt::Display for Live2dSkin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.config_value())
    }
}

impl FromStr for Live2dSkin {
    type Err = UnknownLive2dSkin;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "default" => Ok(Self::Default),
            "pajama" => Ok(Self::Pajama),
            "rain" => Ok(Self::Rain),
            "winter" => Ok(Self::Winter),
            "celebration" => Ok(Self::Celebration),
            "cag_operator" => Ok(Self::CagOperator),
            _ => Err(UnknownLive2dSkin),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownLive2dSkin;

impl fmt::Display for UnknownLive2dSkin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("unknown Live2D skin")
    }
}

impl std::error::Error for UnknownLive2dSkin {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Live2dMotionPriority {
    Idle,
    Normal,
    Force,
}

/// Commands understood by the state bridge regardless of renderer feature flags.
#[derive(Debug, Clone, PartialEq)]
pub enum Live2dCommand {
    LoadBuiltinModel {
        skin: Live2dSkin,
    },
    SetParameter {
        id: &'static str,
        value: f32,
        weight: f32,
    },
    PlayMotion {
        group: &'static str,
        index: usize,
        priority: Live2dMotionPriority,
    },
    SetExpression {
        id: &'static str,
    },
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
}

/// Cubism hit areas normalized to desktop-pet interactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Live2dHitArea {
    Head,
    Body,
    Mic,
}

impl Live2dHitArea {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Head => "Head",
            Self::Body => "Body",
            Self::Mic => "Mic",
        }
    }

    pub const fn action(self) -> Live2dHitAction {
        match self {
            Self::Head => Live2dHitAction::Pet,
            Self::Body => Live2dHitAction::Drag,
            Self::Mic => Live2dHitAction::ToggleListening,
        }
    }
}

impl TryFrom<&str> for Live2dHitArea {
    type Error = UnknownLive2dHitArea;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.trim() {
            "Head" => Ok(Self::Head),
            "Body" => Ok(Self::Body),
            "Mic" => Ok(Self::Mic),
            _ => Err(UnknownLive2dHitArea),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownLive2dHitArea;

impl fmt::Display for UnknownLive2dHitArea {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("unknown Live2D hit area")
    }
}

impl std::error::Error for UnknownLive2dHitArea {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Live2dHitAction {
    Pet,
    Drag,
    ToggleListening,
}

/// Stateful bridge from desktop-pet state to renderer commands.
#[derive(Debug, Clone)]
pub struct Live2dController {
    skin: Live2dSkin,
    state: PetState,
}

impl Default for Live2dController {
    fn default() -> Self {
        Self::new(Live2dSkin::Default)
    }
}

impl Live2dController {
    pub const fn new(skin: Live2dSkin) -> Self {
        Self {
            skin,
            state: PetState::Idle,
        }
    }

    pub const fn skin(&self) -> Live2dSkin {
        self.skin
    }

    pub const fn state(&self) -> PetState {
        self.state
    }

    /// Load the selected model and fully initialize its current visual state.
    pub fn initial_commands(&self) -> Vec<Live2dCommand> {
        let mut commands = Vec::with_capacity(8);
        commands.push(self.load_model_command());
        commands.extend(self.commands_for_state(self.state));
        commands
    }

    pub const fn load_model_command(&self) -> Live2dCommand {
        Live2dCommand::LoadBuiltinModel { skin: self.skin }
    }

    /// Switch model skins and replay state because model parameters are not shared.
    pub fn set_skin(&mut self, skin: Live2dSkin) -> Vec<Live2dCommand> {
        if self.skin == skin {
            return Vec::new();
        }
        self.skin = skin;
        self.initial_commands()
    }

    /// Emit commands only when the logical pet state changes.
    pub fn set_state(&mut self, state: PetState) -> Vec<Live2dCommand> {
        if self.state == state {
            return Vec::new();
        }
        self.state = state;
        self.commands_for_state(state)
    }

    /// Build a complete state snapshot suitable for initial load or resynchronization.
    pub fn commands_for_state(&self, state: PetState) -> Vec<Live2dCommand> {
        let (motion, expression, priority) = state_presentation(self.skin, state);
        let mut commands = Vec::with_capacity(7);
        commands.push(Live2dCommand::PlayMotion {
            group: motion,
            index: 0,
            priority,
        });
        commands.push(Live2dCommand::SetExpression { id: expression });

        let listening = state_flag(state == PetState::Listening);
        let thinking = state_flag(state == PetState::Processing);
        let petted = state_flag(state == PetState::Petted);
        let sleepy = state_flag(state == PetState::Sleepy);
        let tear = state_flag(state == PetState::Error);
        for (id, value) in [
            (self.skin.listening_parameter(), listening),
            (PARAM_THINKING, thinking),
            (PARAM_PET, petted),
            (PARAM_SLEEP, sleepy),
            (PARAM_TEAR, tear),
        ] {
            commands.push(Live2dCommand::SetParameter {
                id,
                value,
                weight: 1.0,
            });
        }
        commands
    }

    pub fn lip_sync(value: f32) -> Live2dCommand {
        Live2dCommand::SetLipSync {
            value: clamp_unit(value),
        }
    }

    pub fn look_at(x: f32, y: f32) -> Live2dCommand {
        Live2dCommand::SetLookAt {
            x: clamp_signed_unit(x),
            y: clamp_signed_unit(y),
        }
    }

    pub fn hit_test(x: f32, y: f32) -> Live2dCommand {
        Live2dCommand::HitTest {
            x: clamp_signed_unit(x),
            y: clamp_signed_unit(y),
        }
    }

    pub fn hit_action(name: &str) -> Option<Live2dHitAction> {
        Live2dHitArea::try_from(name)
            .ok()
            .map(Live2dHitArea::action)
    }
}

fn state_presentation(
    skin: Live2dSkin,
    state: PetState,
) -> (&'static str, &'static str, Live2dMotionPriority) {
    match state {
        PetState::Idle => ("idle", "neutral", Live2dMotionPriority::Idle),
        PetState::Listening => ("listening", "listening", Live2dMotionPriority::Normal),
        PetState::Processing => (
            "processing",
            if skin == Live2dSkin::CagOperator {
                "focused"
            } else {
                "processing"
            },
            Live2dMotionPriority::Normal,
        ),
        PetState::Success => ("success", "success", Live2dMotionPriority::Force),
        PetState::Error => ("error", "error", Live2dMotionPriority::Force),
        PetState::Petted => ("petted", "petted", Live2dMotionPriority::Force),
        PetState::Sleepy => ("sleepy", "sleepy", Live2dMotionPriority::Normal),
    }
}

fn clamp_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

const fn state_flag(enabled: bool) -> f32 {
    if enabled {
        1.0
    } else {
        0.0
    }
}

fn clamp_signed_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

pub type Skin = Live2dSkin;
pub type HitArea = Live2dHitArea;
pub type HitAreaAction = Live2dHitAction;

#[cfg(feature = "webview2-renderer")]
mod webview2 {
    use super::{Live2dCommand, Live2dController, Live2dHitArea, Live2dMotionPriority, Live2dSkin};
    use crate::ui::live2d_web::{BuiltinModel, HostCommand, MotionPriority, RuntimeEvent};

    impl From<Live2dSkin> for BuiltinModel {
        fn from(skin: Live2dSkin) -> Self {
            match skin {
                Live2dSkin::Default => Self::Aiko,
                Live2dSkin::Pajama => Self::AikoPajama,
                Live2dSkin::Rain => Self::AikoRain,
                Live2dSkin::Winter => Self::AikoWinter,
                Live2dSkin::Celebration => Self::AikoCelebration,
                Live2dSkin::CagOperator => Self::AikoCagOperator,
            }
        }
    }

    impl From<Live2dMotionPriority> for MotionPriority {
        fn from(priority: Live2dMotionPriority) -> Self {
            match priority {
                Live2dMotionPriority::Idle => Self::Idle,
                Live2dMotionPriority::Normal => Self::Normal,
                Live2dMotionPriority::Force => Self::Force,
            }
        }
    }

    impl From<Live2dCommand> for HostCommand {
        fn from(command: Live2dCommand) -> Self {
            match command {
                Live2dCommand::LoadBuiltinModel { skin } => {
                    Self::LoadBuiltinModel { model: skin.into() }
                }
                Live2dCommand::SetParameter { id, value, weight } => Self::SetParameter {
                    id: id.to_string(),
                    value,
                    weight,
                },
                Live2dCommand::PlayMotion {
                    group,
                    index,
                    priority,
                } => Self::PlayMotion {
                    group: group.to_string(),
                    index,
                    priority: priority.into(),
                },
                Live2dCommand::SetExpression { id } => Self::SetExpression { id: id.to_string() },
                Live2dCommand::SetLookAt { x, y } => Self::SetLookAt { x, y },
                Live2dCommand::SetLipSync { value } => Self::SetLipSync { value },
                Live2dCommand::HitTest { x, y } => Self::HitTest { x, y },
            }
        }
    }

    impl Live2dController {
        pub fn initial_host_commands(&self) -> Vec<HostCommand> {
            self.initial_commands()
                .into_iter()
                .map(HostCommand::from)
                .collect()
        }

        pub fn host_commands_for_state(&self, state: super::PetState) -> Vec<HostCommand> {
            self.commands_for_state(state)
                .into_iter()
                .map(HostCommand::from)
                .collect()
        }
    }

    impl Live2dHitArea {
        pub fn from_runtime_event(event: &RuntimeEvent) -> Option<Self> {
            match event {
                RuntimeEvent::HitArea { name, .. } => Self::try_from(name.as_str()).ok(),
                _ => None,
            }
        }
    }
}
