use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    Open,
    Back,
    Refresh,
    Quit,
}

/// Snapshot of controls, combining keyboard and all connected gamepads.
#[derive(Default, Clone, Copy)]
pub struct Controls(pub [bool; 6]);

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
    pub fn update(&mut self, controls: Controls, now: Instant) -> Vec<Action> {
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
            Action::Open,
            Action::Back,
            Action::Refresh,
            Action::Quit,
        ];
        let mut result = vec![];
        for (i, action) in actions.iter().enumerate().skip(2) {
            if controls.0[i] && !self.previous.0[i] {
                result.push(*action);
            }
        }
        let direction = match (controls.0[0], controls.0[1]) {
            (true, false) => Some(0),
            (false, true) => Some(1),
            _ => None,
        };
        if let Some(i) = direction {
            if self.direction != direction {
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
        let controls = Controls([false, false, false, true, true, true]);
        assert_eq!(
            input.update(controls, t + Duration::from_millis(210)),
            vec![Action::Back, Action::Refresh, Action::Quit]
        );
        assert!(
            input
                .update(controls, t + Duration::from_secs(1))
                .is_empty()
        );
        let up = Controls([true, false, false, false, false, false]);
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
    fn repeat_edges_and_playback_neutral_gate() {
        let t = Instant::now();
        let mut input = Input::new(t);
        input.update(Controls::default(), t);
        input.update(Controls::default(), t + Duration::from_millis(201));
        let held = Controls([false, true, true, false, false, false]);
        assert_eq!(
            input.update(held, t + Duration::from_millis(210)),
            vec![Action::Open, Action::Down]
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
            vec![Action::Open, Action::Down]
        );
    }
}
