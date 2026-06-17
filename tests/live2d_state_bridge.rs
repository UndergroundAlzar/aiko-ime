use std::collections::BTreeSet;

use aiko_ime::ui::{
    Live2dCommand, Live2dController, Live2dHitAction, Live2dHitArea, Live2dMotionPriority,
    Live2dSkin, PetState,
};

#[test]
fn six_fixed_skins_have_unique_model_ids_and_paths() {
    let ids: BTreeSet<_> = Live2dSkin::ALL
        .into_iter()
        .map(Live2dSkin::model_id)
        .collect();
    let paths: BTreeSet<_> = Live2dSkin::ALL
        .into_iter()
        .map(Live2dSkin::model3_relative_path)
        .collect();

    assert_eq!(ids.len(), 6);
    assert_eq!(paths.len(), 6);
    for skin in Live2dSkin::ALL {
        assert_eq!(skin.config_value().parse(), Ok(skin));
        assert!(skin.model_id().starts_with("aiko"));
        assert!(skin.model3_relative_path().starts_with("assets/live2d/"));
        assert!(skin.model3_relative_path().ends_with(".model3.json"));
    }
}

#[test]
fn every_pet_state_maps_to_motion_expression_and_parameter_snapshot() {
    let controller = Live2dController::default();
    let states = [
        PetState::Idle,
        PetState::Listening,
        PetState::Processing,
        PetState::Success,
        PetState::Error,
        PetState::Petted,
        PetState::Sleepy,
    ];

    for state in states {
        let commands = controller.commands_for_state(state);
        assert_eq!(commands.len(), 7);
        assert!(matches!(commands[0], Live2dCommand::PlayMotion { .. }));
        assert!(matches!(commands[1], Live2dCommand::SetExpression { .. }));
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(command, Live2dCommand::SetParameter { .. }))
                .count(),
            5
        );
    }
}

#[test]
fn state_specific_commands_select_expected_drivers() {
    let controller = Live2dController::default();
    assert!(controller
        .commands_for_state(PetState::Idle)
        .contains(&Live2dCommand::PlayMotion {
            group: "idle",
            index: 0,
            priority: Live2dMotionPriority::Idle,
        }));
    assert!(controller.commands_for_state(PetState::Listening).contains(
        &Live2dCommand::SetParameter {
            id: "ParamAikoMicGlow",
            value: 1.0,
            weight: 1.0,
        }
    ));
    assert!(controller.commands_for_state(PetState::Error).contains(
        &Live2dCommand::SetParameter {
            id: "ParamAikoTear",
            value: 1.0,
            weight: 1.0,
        }
    ));

    let cag = Live2dController::new(Live2dSkin::CagOperator);
    assert!(cag
        .commands_for_state(PetState::Listening)
        .contains(&Live2dCommand::SetParameter {
            id: "ParamAikoListening",
            value: 1.0,
            weight: 1.0,
        }));
    assert!(cag
        .commands_for_state(PetState::Processing)
        .contains(&Live2dCommand::SetExpression { id: "focused" }));
}

#[test]
fn lip_sync_look_at_and_hit_test_are_safely_normalized() {
    assert_eq!(
        Live2dController::lip_sync(1.5),
        Live2dCommand::SetLipSync { value: 1.0 }
    );
    assert_eq!(
        Live2dController::lip_sync(f32::NAN),
        Live2dCommand::SetLipSync { value: 0.0 }
    );
    assert_eq!(
        Live2dController::look_at(-2.0, 2.0),
        Live2dCommand::SetLookAt { x: -1.0, y: 1.0 }
    );
    assert_eq!(
        Live2dController::hit_test(f32::INFINITY, -0.25),
        Live2dCommand::HitTest { x: 0.0, y: -0.25 }
    );
}

#[test]
fn hit_areas_convert_to_desktop_pet_actions() {
    assert_eq!(
        Live2dHitArea::try_from("Head").unwrap().action(),
        Live2dHitAction::Pet
    );
    assert_eq!(
        Live2dHitArea::try_from("Body").unwrap().action(),
        Live2dHitAction::Drag
    );
    assert_eq!(
        Live2dController::hit_action("Mic"),
        Some(Live2dHitAction::ToggleListening)
    );
    assert_eq!(Live2dController::hit_action("Unknown"), None);
}

#[cfg(feature = "webview2-renderer")]
#[test]
fn bridge_commands_convert_to_existing_host_protocol() {
    use aiko_ime::ui::live2d_web::{BuiltinModel, HostCommand, RuntimeEvent};

    let controller = Live2dController::new(Live2dSkin::Celebration);
    assert!(matches!(
        HostCommand::from(controller.load_model_command()),
        HostCommand::LoadBuiltinModel {
            model: BuiltinModel::AikoCelebration
        }
    ));
    assert!(controller
        .host_commands_for_state(PetState::Success)
        .iter()
        .any(|command| matches!(
            command,
            HostCommand::PlayMotion { group, .. } if group == "success"
        )));

    let event = RuntimeEvent::HitArea {
        name: "Head".to_string(),
        x: 0.0,
        y: 0.5,
    };
    assert_eq!(
        Live2dHitArea::from_runtime_event(&event),
        Some(Live2dHitArea::Head)
    );
}
