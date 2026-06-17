use std::path::PathBuf;

use super::error::Live2dError;
use super::manifest::ModelManifest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionPriority {
    Idle,
    Normal,
    Force,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Live2dCommand {
    PlayMotion {
        group: String,
        index: usize,
        priority: MotionPriority,
    },
    SetExpression {
        id: String,
    },
    StopAllMotions,
    ResetExpression,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedCommand {
    PlayMotion {
        group: String,
        index: usize,
        file: PathBuf,
        sound: Option<PathBuf>,
        fade_in_seconds: Option<f32>,
        fade_out_seconds: Option<f32>,
        priority: MotionPriority,
    },
    SetExpression {
        id: String,
        file: PathBuf,
    },
    StopAllMotions,
    ResetExpression,
}

pub fn resolve_command(
    manifest: &ModelManifest,
    command: Live2dCommand,
) -> Result<ResolvedCommand, Live2dError> {
    match command {
        Live2dCommand::PlayMotion {
            group,
            index,
            priority,
        } => {
            let entries =
                manifest
                    .motions
                    .get(&group)
                    .ok_or_else(|| Live2dError::UnknownMotionGroup {
                        group: group.clone(),
                    })?;
            let motion = entries
                .get(index)
                .ok_or_else(|| Live2dError::MotionIndexOutOfRange {
                    group: group.clone(),
                    index,
                })?;
            Ok(ResolvedCommand::PlayMotion {
                group,
                index,
                file: motion.file.clone(),
                sound: motion.sound.clone(),
                fade_in_seconds: motion.fade_in_seconds,
                fade_out_seconds: motion.fade_out_seconds,
                priority,
            })
        }
        Live2dCommand::SetExpression { id } => {
            let expression = manifest
                .expression(&id)
                .ok_or_else(|| Live2dError::UnknownExpression { id: id.clone() })?;
            Ok(ResolvedCommand::SetExpression {
                id,
                file: expression.file.clone(),
            })
        }
        Live2dCommand::StopAllMotions => Ok(ResolvedCommand::StopAllMotions),
        Live2dCommand::ResetExpression => Ok(ResolvedCommand::ResetExpression),
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_command, Live2dCommand, MotionPriority, ResolvedCommand};
    use crate::ui::live2d::manifest::ModelManifest;
    use crate::ui::live2d::test_support::{write_minimal_model, TestDir};

    #[test]
    fn resolves_motion_and_expression_to_manifest_assets() {
        let temp = TestDir::new("commands");
        let manifest =
            ModelManifest::load(write_minimal_model(temp.path())).expect("valid manifest");

        let motion = resolve_command(
            &manifest,
            Live2dCommand::PlayMotion {
                group: "Idle".to_string(),
                index: 0,
                priority: MotionPriority::Idle,
            },
        )
        .unwrap();
        assert!(matches!(
            motion,
            ResolvedCommand::PlayMotion { ref file, .. }
                if file.ends_with("idle.motion3.json")
        ));

        let expression = resolve_command(
            &manifest,
            Live2dCommand::SetExpression {
                id: "smile".to_string(),
            },
        )
        .unwrap();
        assert!(matches!(
            expression,
            ResolvedCommand::SetExpression { ref file, .. }
                if file.ends_with("smile.exp3.json")
        ));
    }

    #[test]
    fn rejects_unknown_motion_and_expression() {
        let temp = TestDir::new("commands-invalid");
        let manifest =
            ModelManifest::load(write_minimal_model(temp.path())).expect("valid manifest");

        let motion = resolve_command(
            &manifest,
            Live2dCommand::PlayMotion {
                group: "Missing".to_string(),
                index: 0,
                priority: MotionPriority::Normal,
            },
        )
        .unwrap_err();
        assert!(motion.to_string().contains("unknown Cubism motion group"));

        let expression = resolve_command(
            &manifest,
            Live2dCommand::SetExpression {
                id: "missing".to_string(),
            },
        )
        .unwrap_err();
        assert!(expression.to_string().contains("unknown Cubism expression"));
    }
}
