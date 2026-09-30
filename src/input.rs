use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    Confirm,
    Back,
    Refresh,
    Menu,
    Quit,
    Mute,
    Stop,
    Seek(i32),
    Volume(i32),
    Left,
    Right,
    PlayPause,
    Fullscreen,
}

/// Snapshot of controls, combining keyboard and all connected gamepads.
#[derive(Default, Clone, Copy)]
pub struct Controls(pub [bool; 12]);

pub struct Input {
    previous: Controls,
    blocked: bool,
    neutral_since: Option<Instant>,
    direction: Option<usize>,
    repeat_at: Instant,
}
impl Input {
    pub fn new(now: Instant) -> Self {
        Self {
            previous: Controls::default(),
            blocked: true,
            neutral_since: None,
            direction: None,
            repeat_at: now,
        }
    }
    pub fn waiting_for_neutral(&self) -> bool {
        self.blocked
    }
    pub fn block(&mut self) {
        self.blocked = true;
        self.neutral_since = None;
        self.direction = None;
    }
    #[cfg(test)]
    pub fn update(&mut self, controls: Controls, now: Instant) -> Vec<Action> {
        self.update_with_edges(controls, Controls::default(), now)
    }
    pub fn update_with_edges(
        &mut self,
        controls: Controls,
        pressed: Controls,
        now: Instant,
    ) -> Vec<Action> {
        if self.blocked {
            if controls.0.iter().any(|v| *v) {
                self.neutral_since = None;
            } else if now.duration_since(*self.neutral_since.get_or_insert(now))
                >= Duration::from_millis(200)
            {
                self.blocked = false;
            }
            self.previous = controls;
            return vec![];
        }
        let actions = [
            Action::Up,
            Action::Down,
            Action::Confirm,
            Action::Back,
            Action::Refresh,
            Action::Menu,
            Action::Left,
            Action::Right,
            Action::PlayPause,
            Action::Fullscreen,
            Action::Quit,
            Action::Mute,
        ];
        let mut result = vec![];
        for (i, action) in actions.iter().enumerate().skip(2) {
            if pressed.0[i] || (controls.0[i] && !self.previous.0[i]) {
                result.push(*action);
            }
        }
        let direction = match (controls.0[0], controls.0[1]) {
            (true, false) => Some(0),
            (false, true) => Some(1),
            _ => None,
        };
        if let Some(i) = direction {
            if self.direction != direction || pressed.0[i] {
                result.push(actions[i]);
                self.repeat_at = now + Duration::from_millis(350);
            } else if now >= self.repeat_at {
                result.push(actions[i]);
                self.repeat_at = now + Duration::from_millis(100);
            }
        }
        self.direction = direction;
        self.previous = controls;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actions_are_once_per_press_and_disconnect_clears_repeat() {
        let t = Instant::now();
        let mut input = Input::new(t);
        input.update(Controls::default(), t);
        input.update(Controls::default(), t + Duration::from_millis(201));
        let controls = Controls([
            false, false, false, true, true, true, false, false, false, false, false, false,
        ]);
        assert_eq!(
            input.update(controls, t + Duration::from_millis(210)),
            vec![Action::Back, Action::Refresh, Action::Menu]
        );
        assert!(
            input
                .update(controls, t + Duration::from_secs(1))
                .is_empty()
        );
        let up = Controls([
            true, false, false, false, false, false, false, false, false, false, false, false,
        ]);
        assert_eq!(
            input.update(up, t + Duration::from_secs(2)),
            vec![Action::Up]
        );
        // A disconnected controller contributes an all-neutral snapshot.
        assert!(
            input
                .update(Controls::default(), t + Duration::from_secs(3))
                .is_empty()
        );
        assert_eq!(
            input.update(up, t + Duration::from_secs(4)),
            vec![Action::Up]
        );
        assert!(input.update(up, t + Duration::from_millis(4100)).is_empty());
    }
    #[test]
    fn playback_keys_share_edges_and_neutral_gating() {
        let t = Instant::now();
        let mut input = Input::new(t);
        let held = Controls([
            false, false, false, false, false, false, true, true, true, true, false, false,
        ]);
        assert!(input.update(held, t).is_empty());
        assert!(input.update(held, t + Duration::from_secs(1)).is_empty());
        input.update(Controls::default(), t + Duration::from_secs(2));
        input.update(Controls::default(), t + Duration::from_millis(2201));
        assert_eq!(
            input.update(held, t + Duration::from_millis(2210)),
            vec![
                Action::Left,
                Action::Right,
                Action::PlayPause,
                Action::Fullscreen
            ]
        );
        assert!(input.update(held, t + Duration::from_secs(3)).is_empty());
    }
    #[test]
    fn repeat_edges_and_playback_neutral_gate() {
        let t = Instant::now();
        let mut input = Input::new(t);
        input.update(Controls::default(), t);
        input.update(Controls::default(), t + Duration::from_millis(201));
        let held = Controls([
            false, true, true, false, false, false, false, false, false, false, false, false,
        ]);
        assert_eq!(
            input.update(held, t + Duration::from_millis(210)),
            vec![Action::Confirm, Action::Down]
        );
        assert!(
            input
                .update(held, t + Duration::from_millis(559))
                .is_empty()
        );
        assert_eq!(
            input.update(held, t + Duration::from_millis(560)),
            vec![Action::Down]
        );
        assert_eq!(
            input.update(held, t + Duration::from_millis(660)),
            vec![Action::Down]
        );
        input.block();
        assert!(input.update(held, t + Duration::from_secs(2)).is_empty());
        input.update(Controls::default(), t + Duration::from_secs(3));
        assert!(
            input
                .update(held, t + Duration::from_millis(3100))
                .is_empty()
        );
        input.update(Controls::default(), t + Duration::from_secs(4));
        input.update(Controls::default(), t + Duration::from_millis(4201));
        assert_eq!(
            input.update(held, t + Duration::from_millis(4210)),
            vec![Action::Confirm, Action::Down]
        );
    }
    #[test]
    fn fresh_edges_survive_a_release_and_repress_between_frames() {
        let now = Instant::now();
        let mut input = Input::new(now);
        input.update(Controls::default(), now);
        input.update(Controls::default(), now + Duration::from_millis(201));
        let mut held = Controls::default();
        held.0[2] = true;
        assert_eq!(
            input.update(held, now + Duration::from_millis(210)),
            [Action::Confirm]
        );
        assert!(
            input
                .update(held, now + Duration::from_millis(250))
                .is_empty()
        );
        assert_eq!(
            input.update_with_edges(held, held, now + Duration::from_millis(300)),
            [Action::Confirm]
        );
    }
}
