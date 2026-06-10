//! Frame animation primitives for the desktop pet.

pub const SPRITE_IDLE: u16 = 0;
pub const SPRITE_LISTENING: u16 = 1;
pub const SPRITE_PROCESSING: u16 = 2;
pub const SPRITE_SUCCESS: u16 = 3;
pub const SPRITE_ERROR: u16 = 4;
pub const SPRITE_PETTED: u16 = 5;
pub const SPRITE_SLEEPY: u16 = 6;

/// A single transform frame applied to the current Aiko image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PetAnimationFrame {
    pub duration_ms: u32,
    pub sprite_index: u16,
    pub offset_x: i16,
    pub offset_y: i16,
    pub scale_permille: u16,
    pub opacity: u8,
}

impl PetAnimationFrame {
    pub const fn new(
        duration_ms: u32,
        offset_x: i16,
        offset_y: i16,
        scale_permille: u16,
        opacity: u8,
    ) -> Self {
        Self {
            duration_ms,
            sprite_index: 0,
            offset_x,
            offset_y,
            scale_permille,
            opacity,
        }
    }

    pub const fn with_sprite(mut self, sprite_index: u16) -> Self {
        self.sprite_index = sprite_index;
        self
    }
}

impl Default for PetAnimationFrame {
    fn default() -> Self {
        Self::new(250, 0, 0, 1_000, 255)
    }
}

/// Raw RGBA image used by one or more animation frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PetFrameImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl PetFrameImage {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Option<Self> {
        let image = Self {
            width,
            height,
            rgba,
        };
        if !image.is_valid() {
            return None;
        }
        Some(image)
    }

    pub fn is_valid(&self) -> bool {
        let expected_len = self
            .width
            .checked_mul(self.height)
            .and_then(|pixels| pixels.checked_mul(4))
            .map(|bytes| bytes as usize);
        self.width > 0 && self.height > 0 && expected_len == Some(self.rgba.len())
    }
}

/// A reusable sequence of transform frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PetAnimationClip {
    pub frames: Vec<PetAnimationFrame>,
    pub looping: bool,
}

impl PetAnimationClip {
    pub fn new(frames: Vec<PetAnimationFrame>, looping: bool) -> Self {
        let frames = if frames.is_empty() {
            vec![PetAnimationFrame::default()]
        } else {
            frames
        };
        Self { frames, looping }
    }

    pub fn idle() -> Self {
        Self::new(
            vec![
                PetAnimationFrame::new(520, 0, 1, 998, 255).with_sprite(SPRITE_IDLE),
                PetAnimationFrame::new(520, 0, -1, 1_006, 255).with_sprite(SPRITE_IDLE),
                PetAnimationFrame::new(520, 1, 0, 1_003, 255).with_sprite(SPRITE_IDLE),
                PetAnimationFrame::new(520, -1, 1, 998, 255).with_sprite(SPRITE_IDLE),
            ],
            true,
        )
    }

    pub fn listening() -> Self {
        Self::new(
            vec![
                PetAnimationFrame::new(120, 0, 0, 990, 255).with_sprite(SPRITE_LISTENING),
                PetAnimationFrame::new(120, 0, -3, 1_025, 255).with_sprite(SPRITE_LISTENING),
                PetAnimationFrame::new(120, 1, -1, 1_010, 255).with_sprite(SPRITE_LISTENING),
                PetAnimationFrame::new(120, -1, 0, 1_000, 255).with_sprite(SPRITE_LISTENING),
            ],
            true,
        )
    }

    pub fn processing() -> Self {
        Self::new(
            vec![
                PetAnimationFrame::new(95, -2, 0, 1_000, 245).with_sprite(SPRITE_PROCESSING),
                PetAnimationFrame::new(95, 0, -1, 1_012, 255).with_sprite(SPRITE_PROCESSING),
                PetAnimationFrame::new(95, 2, 0, 1_000, 245).with_sprite(SPRITE_PROCESSING),
                PetAnimationFrame::new(95, 0, 1, 990, 255).with_sprite(SPRITE_PROCESSING),
            ],
            true,
        )
    }

    pub fn success() -> Self {
        Self::new(
            vec![
                PetAnimationFrame::new(70, 0, 2, 970, 255).with_sprite(SPRITE_SUCCESS),
                PetAnimationFrame::new(80, 0, -5, 1_050, 255).with_sprite(SPRITE_SUCCESS),
                PetAnimationFrame::new(90, 1, -2, 1_025, 255).with_sprite(SPRITE_SUCCESS),
                PetAnimationFrame::new(180, 0, 0, 1_000, 255).with_sprite(SPRITE_SUCCESS),
            ],
            false,
        )
    }

    pub fn error() -> Self {
        Self::new(
            vec![
                PetAnimationFrame::new(75, -4, 0, 1_000, 255).with_sprite(SPRITE_ERROR),
                PetAnimationFrame::new(75, 4, 0, 1_000, 255).with_sprite(SPRITE_ERROR),
                PetAnimationFrame::new(75, -3, 0, 1_000, 255).with_sprite(SPRITE_ERROR),
                PetAnimationFrame::new(75, 3, 0, 1_000, 255).with_sprite(SPRITE_ERROR),
                PetAnimationFrame::new(250, 0, 0, 1_000, 235).with_sprite(SPRITE_ERROR),
            ],
            false,
        )
    }

    pub fn petted() -> Self {
        Self::new(
            vec![
                PetAnimationFrame::new(120, 0, 0, 1_020, 255).with_sprite(SPRITE_PETTED),
                PetAnimationFrame::new(120, -1, -2, 1_035, 255).with_sprite(SPRITE_PETTED),
                PetAnimationFrame::new(160, 1, 0, 1_020, 255).with_sprite(SPRITE_PETTED),
            ],
            true,
        )
    }

    pub fn sleepy() -> Self {
        Self::new(
            vec![
                PetAnimationFrame::new(680, 0, 2, 990, 235).with_sprite(SPRITE_SLEEPY),
                PetAnimationFrame::new(680, 0, 3, 985, 225).with_sprite(SPRITE_SLEEPY),
            ],
            true,
        )
    }

    pub fn happy() -> Self {
        Self::new(
            vec![
                PetAnimationFrame::new(55, 0, 1, 980, 255).with_sprite(SPRITE_PETTED),
                PetAnimationFrame::new(65, -2, -6, 1_060, 255).with_sprite(SPRITE_PETTED),
                PetAnimationFrame::new(65, 2, -5, 1_050, 255).with_sprite(SPRITE_PETTED),
                PetAnimationFrame::new(80, 0, -2, 1_025, 255).with_sprite(SPRITE_PETTED),
                PetAnimationFrame::new(130, 0, 0, 1_000, 255).with_sprite(SPRITE_PETTED),
            ],
            false,
        )
    }
}

/// Deterministic cursor for a [`PetAnimationClip`].
#[derive(Debug, Clone)]
pub struct PetAnimationPlayer {
    clip: PetAnimationClip,
    frame_index: usize,
    elapsed_in_frame_ms: u32,
    completed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PetAnimationAdvance {
    pub frame_changed: bool,
    pub completed: bool,
}

impl PetAnimationPlayer {
    pub fn new(clip: PetAnimationClip) -> Self {
        Self {
            clip,
            frame_index: 0,
            elapsed_in_frame_ms: 0,
            completed: false,
        }
    }

    pub fn set_clip(&mut self, clip: PetAnimationClip) {
        *self = Self::new(clip);
    }

    pub fn current_frame(&self) -> PetAnimationFrame {
        self.clip.frames[self.frame_index]
    }

    pub fn is_completed(&self) -> bool {
        self.completed
    }

    pub fn remaining_in_frame_ms(&self) -> u32 {
        self.current_frame()
            .duration_ms
            .max(1)
            .saturating_sub(self.elapsed_in_frame_ms)
            .max(1)
    }

    pub fn advance(&mut self, elapsed_ms: u32) -> PetAnimationAdvance {
        if self.completed || elapsed_ms == 0 {
            return PetAnimationAdvance {
                frame_changed: false,
                completed: self.completed,
            };
        }

        let mut remaining = elapsed_ms;
        let mut frame_changed = false;
        while remaining > 0 && !self.completed {
            let frame_remaining = self.remaining_in_frame_ms();
            if remaining < frame_remaining {
                self.elapsed_in_frame_ms += remaining;
                break;
            }

            remaining -= frame_remaining;
            self.elapsed_in_frame_ms = 0;
            if self.frame_index + 1 < self.clip.frames.len() {
                self.frame_index += 1;
                frame_changed = true;
            } else if self.clip.looping {
                self.frame_index = 0;
                frame_changed = true;
            } else {
                self.completed = true;
            }
        }

        PetAnimationAdvance {
            frame_changed,
            completed: self.completed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looping_clip_wraps_to_first_frame() {
        let clip = PetAnimationClip::new(
            vec![
                PetAnimationFrame::new(10, 0, 0, 1_000, 255),
                PetAnimationFrame::new(10, 2, 0, 1_000, 255),
            ],
            true,
        );
        let mut player = PetAnimationPlayer::new(clip);

        let advance = player.advance(20);

        assert!(advance.frame_changed);
        assert!(!advance.completed);
        assert_eq!(player.current_frame().offset_x, 0);
    }

    #[test]
    fn one_shot_clip_stays_on_last_frame() {
        let clip = PetAnimationClip::new(
            vec![
                PetAnimationFrame::new(10, 0, 0, 1_000, 255),
                PetAnimationFrame::new(10, 4, 0, 1_000, 255),
            ],
            false,
        );
        let mut player = PetAnimationPlayer::new(clip);

        let advance = player.advance(25);

        assert!(advance.completed);
        assert!(player.is_completed());
        assert_eq!(player.current_frame().offset_x, 4);
    }

    #[test]
    fn frame_image_rejects_invalid_rgba_length() {
        assert!(PetFrameImage::new(2, 2, vec![0; 15]).is_none());
        assert!(PetFrameImage::new(2, 2, vec![0; 16]).is_some());
    }
}
