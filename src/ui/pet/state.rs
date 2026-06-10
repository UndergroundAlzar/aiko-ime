//! Pure desktop-pet interaction state machine.

use super::animation::PetAnimationClip;

const SUCCESS_HOLD_MS: u32 = 1_800;
const ERROR_HOLD_MS: u32 = 2_500;
const PETTED_HOLD_MS: u32 = 900;
const SLEEPY_IDLE_MS: u32 = 45_000;
const PET_DISTANCE_THRESHOLD: f32 = 72.0;

/// User-visible activity state for Aiko.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetState {
    Idle,
    Listening,
    Processing,
    Success,
    Error,
    Petted,
    Sleepy,
}

impl PetState {
    pub fn animation(self) -> PetAnimationClip {
        match self {
            Self::Idle => PetAnimationClip::idle(),
            Self::Listening => PetAnimationClip::listening(),
            Self::Processing => PetAnimationClip::processing(),
            Self::Success => PetAnimationClip::success(),
            Self::Error => PetAnimationClip::error(),
            Self::Petted => PetAnimationClip::petted(),
            Self::Sleepy => PetAnimationClip::sleepy(),
        }
    }
}

/// Effects emitted by [`PetStateMachine`] for the window/event bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetAction {
    StateChanged(PetState),
    StartListeningRequested,
    StopListeningRequested,
    HoverChanged(bool),
    HappyFeedback { pet_count: u32 },
}

/// Platform-independent state and gesture recognition for the desktop pet.
#[derive(Debug, Clone)]
pub struct PetStateMachine {
    state: PetState,
    interactions_enabled: bool,
    hovered: bool,
    state_elapsed_ms: u32,
    petted_return_state: Option<PetState>,
    last_pointer: Option<(i32, i32)>,
    pet_distance: f32,
    pet_count: u32,
}

impl Default for PetStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl PetStateMachine {
    pub fn new() -> Self {
        Self {
            state: PetState::Idle,
            interactions_enabled: true,
            hovered: false,
            state_elapsed_ms: 0,
            petted_return_state: None,
            last_pointer: None,
            pet_distance: 0.0,
            pet_count: 0,
        }
    }

    pub fn state(&self) -> PetState {
        self.state
    }

    pub fn interactions_enabled(&self) -> bool {
        self.interactions_enabled
    }

    pub fn is_hovered(&self) -> bool {
        self.hovered
    }

    pub fn set_state(&mut self, state: PetState) -> Vec<PetAction> {
        if self.state == state {
            return Vec::new();
        }
        self.state = state;
        self.state_elapsed_ms = 0;
        self.petted_return_state = None;
        vec![PetAction::StateChanged(state)]
    }

    pub fn set_interactions_enabled(&mut self, enabled: bool) -> Vec<PetAction> {
        if self.interactions_enabled == enabled {
            return Vec::new();
        }
        self.interactions_enabled = enabled;
        self.last_pointer = None;
        self.pet_distance = 0.0;

        if !enabled && self.hovered {
            self.hovered = false;
            vec![PetAction::HoverChanged(false)]
        } else {
            Vec::new()
        }
    }

    pub fn pointer_entered(&mut self) -> Vec<PetAction> {
        if !self.interactions_enabled || self.hovered {
            return Vec::new();
        }
        self.hovered = true;
        self.last_pointer = None;
        self.note_activity();

        let mut actions = Vec::with_capacity(2);
        if self.state == PetState::Sleepy {
            self.state = PetState::Idle;
            self.state_elapsed_ms = 0;
            actions.push(PetAction::StateChanged(PetState::Idle));
        }
        actions.push(PetAction::HoverChanged(true));
        actions
    }

    pub fn pointer_left(&mut self) -> Vec<PetAction> {
        self.last_pointer = None;
        self.pet_distance = 0.0;
        if !self.hovered {
            return Vec::new();
        }
        self.hovered = false;
        vec![PetAction::HoverChanged(false)]
    }

    /// Track cursor travel over Aiko. Repeated movement is treated as petting.
    pub fn pointer_moved(&mut self, x: i32, y: i32) -> Vec<PetAction> {
        if !self.interactions_enabled || !self.hovered {
            self.last_pointer = Some((x, y));
            return Vec::new();
        }
        self.note_activity();

        if let Some((last_x, last_y)) = self.last_pointer {
            let dx = (x - last_x) as f32;
            let dy = (y - last_y) as f32;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance <= 48.0 {
                self.pet_distance += distance;
            }
        }
        self.last_pointer = Some((x, y));

        if self.pet_distance >= PET_DISTANCE_THRESHOLD {
            self.pet_distance %= PET_DISTANCE_THRESHOLD;
            let mut actions = Vec::with_capacity(2);
            self.enter_petted(&mut actions);
            return actions;
        }
        Vec::new()
    }

    /// A primary click both toggles listening intent and gives Aiko affection.
    pub fn primary_clicked(&mut self) -> Vec<PetAction> {
        if !self.interactions_enabled {
            return Vec::new();
        }

        let mut actions = Vec::with_capacity(3);
        self.note_activity();
        match self.effective_state() {
            PetState::Idle | PetState::Success | PetState::Error | PetState::Sleepy => {
                self.state = PetState::Listening;
                self.state_elapsed_ms = 0;
                self.petted_return_state = None;
                actions.push(PetAction::StateChanged(PetState::Listening));
                actions.push(PetAction::StartListeningRequested);
            }
            PetState::Listening => {
                self.state = PetState::Processing;
                self.state_elapsed_ms = 0;
                self.petted_return_state = None;
                actions.push(PetAction::StateChanged(PetState::Processing));
                actions.push(PetAction::StopListeningRequested);
            }
            PetState::Processing | PetState::Petted => {}
        }
        self.enter_petted(&mut actions);
        actions
    }

    pub fn pet(&mut self) -> Vec<PetAction> {
        self.note_activity();
        let mut actions = Vec::with_capacity(2);
        self.enter_petted(&mut actions);
        actions
    }

    pub fn tick(&mut self, elapsed_ms: u32) -> Vec<PetAction> {
        self.state_elapsed_ms = self.state_elapsed_ms.saturating_add(elapsed_ms);
        let timeout = match self.state {
            PetState::Success => Some(SUCCESS_HOLD_MS),
            PetState::Error => Some(ERROR_HOLD_MS),
            PetState::Petted => Some(PETTED_HOLD_MS),
            _ => None,
        };

        if timeout.is_some_and(|timeout| self.state_elapsed_ms >= timeout) {
            self.state = if self.state == PetState::Petted {
                self.petted_return_state.take().unwrap_or(PetState::Idle)
            } else {
                PetState::Idle
            };
            self.state_elapsed_ms = 0;
            vec![PetAction::StateChanged(self.state)]
        } else if self.state == PetState::Idle && self.state_elapsed_ms >= SLEEPY_IDLE_MS {
            self.state = PetState::Sleepy;
            self.state_elapsed_ms = 0;
            vec![PetAction::StateChanged(PetState::Sleepy)]
        } else {
            Vec::new()
        }
    }

    fn effective_state(&self) -> PetState {
        if self.state == PetState::Petted {
            self.petted_return_state.unwrap_or(PetState::Idle)
        } else {
            self.state
        }
    }

    fn enter_petted(&mut self, actions: &mut Vec<PetAction>) {
        let return_state = match self.effective_state() {
            PetState::Petted | PetState::Sleepy => PetState::Idle,
            state => state,
        };
        self.state = PetState::Petted;
        self.state_elapsed_ms = 0;
        self.petted_return_state = Some(return_state);
        actions.push(PetAction::StateChanged(PetState::Petted));
        actions.push(self.happy_feedback());
    }

    fn happy_feedback(&mut self) -> PetAction {
        self.pet_count = self.pet_count.saturating_add(1);
        PetAction::HappyFeedback {
            pet_count: self.pet_count,
        }
    }

    fn note_activity(&mut self) {
        if matches!(self.state, PetState::Idle | PetState::Sleepy) {
            self.state_elapsed_ms = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_toggles_listening_then_processing() {
        let mut machine = PetStateMachine::new();

        let start = machine.primary_clicked();
        assert_eq!(machine.state(), PetState::Petted);
        assert!(start.contains(&PetAction::StartListeningRequested));
        assert!(start.contains(&PetAction::StateChanged(PetState::Listening)));
        assert!(start.contains(&PetAction::StateChanged(PetState::Petted)));
        assert!(start.contains(&PetAction::HappyFeedback { pet_count: 1 }));
        assert_eq!(
            machine.tick(PETTED_HOLD_MS),
            vec![PetAction::StateChanged(PetState::Listening)]
        );

        let stop = machine.primary_clicked();
        assert_eq!(machine.state(), PetState::Petted);
        assert!(stop.contains(&PetAction::StopListeningRequested));
        assert!(stop.contains(&PetAction::HappyFeedback { pet_count: 2 }));
        assert_eq!(
            machine.tick(PETTED_HOLD_MS),
            vec![PetAction::StateChanged(PetState::Processing)]
        );
    }

    #[test]
    fn disabled_interactions_ignore_pointer_and_clicks() {
        let mut machine = PetStateMachine::new();
        machine.pointer_entered();
        let disabled = machine.set_interactions_enabled(false);

        assert_eq!(disabled, vec![PetAction::HoverChanged(false)]);
        assert!(machine.primary_clicked().is_empty());
        assert!(machine.pointer_moved(100, 100).is_empty());
        assert_eq!(machine.state(), PetState::Idle);
    }

    #[test]
    fn cursor_travel_triggers_happy_feedback() {
        let mut machine = PetStateMachine::new();
        machine.pointer_entered();
        machine.pointer_moved(0, 0);
        assert!(machine.pointer_moved(40, 0).is_empty());

        let actions = machine.pointer_moved(80, 0);

        assert_eq!(
            actions,
            vec![
                PetAction::StateChanged(PetState::Petted),
                PetAction::HappyFeedback { pet_count: 1 }
            ]
        );
    }

    #[test]
    fn success_and_error_return_to_idle_after_hold() {
        let mut machine = PetStateMachine::new();
        machine.set_state(PetState::Success);
        assert!(machine.tick(SUCCESS_HOLD_MS - 1).is_empty());
        assert_eq!(
            machine.tick(1),
            vec![PetAction::StateChanged(PetState::Idle)]
        );

        machine.set_state(PetState::Error);
        assert!(machine.tick(ERROR_HOLD_MS - 1).is_empty());
        assert_eq!(
            machine.tick(1),
            vec![PetAction::StateChanged(PetState::Idle)]
        );
    }

    #[test]
    fn idle_goes_sleepy_and_hover_wakes_aiko() {
        let mut machine = PetStateMachine::new();

        assert_eq!(
            machine.tick(SLEEPY_IDLE_MS),
            vec![PetAction::StateChanged(PetState::Sleepy)]
        );
        assert_eq!(machine.state(), PetState::Sleepy);
        assert_eq!(
            machine.pointer_entered(),
            vec![
                PetAction::StateChanged(PetState::Idle),
                PetAction::HoverChanged(true)
            ]
        );
    }
}
