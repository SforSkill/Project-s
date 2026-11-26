//! Turns timestamped input (button transitions, mouse view) into `UserCmd`s
//! with sub-tick precision: each transition keeps its position inside the tick
//! and the view angles at that instant (exact first shot).

use engine_core::ViewAngles;
use engine_core::cmd::{ButtonEvent, Buttons, MAX_EVENTS_PER_CMD, UserCmd};

#[derive(Debug, Clone, Default)]
pub struct InputTimeline {
    buttons: Buttons,
    view: ViewAngles,
    /// Transitions not yet put in a command: (time, buttons after, view).
    pending: Vec<(f64, Buttons, ViewAngles)>,
    /// View samples (time, view) since the last command.
    views: Vec<(f64, ViewAngles)>,
    last_buttons: Buttons,
    last_view: ViewAngles,
    weapon_select: u8,
}

impl InputTimeline {
    pub fn view(&self) -> ViewAngles {
        self.view
    }

    pub fn buttons(&self) -> Buttons {
        self.buttons
    }

    /// Mouse motion applied at `t`.
    pub fn set_view(&mut self, t: f64, view: ViewAngles) {
        self.view = view.normalized();
        self.views.push((t, self.view));
        if self.views.len() > 512 {
            self.views.drain(..256);
        }
    }

    /// Resets the view (spawn, server teleport) without interpolation.
    pub fn reset_view(&mut self, view: ViewAngles) {
        self.view = view.normalized();
        self.last_view = self.view;
        self.views.clear();
    }

    /// Button state change at `t`.
    pub fn set_buttons(&mut self, t: f64, b: Buttons) {
        if b != self.buttons {
            self.buttons = b;
            self.pending.push((t, b, self.view));
        }
    }

    /// Requests a weapon slot for the next command (1..=5, 6 = last weapon).
    pub fn select(&mut self, slot: u8) {
        self.weapon_select = slot;
    }

    fn view_at(&self, t: f64) -> ViewAngles {
        let mut prev = self.last_view;
        for (vt, v) in &self.views {
            if *vt > t {
                return prev;
            }
            prev = *v;
        }
        prev
    }

    /// Builds the command covering wall time `[start, end)`.
    pub fn build(&mut self, start: f64, end: f64) -> UserCmd {
        let span = (end - start).max(1e-6);
        let mut events = Vec::new();
        let mut rest = Vec::new();
        for (t, b, v) in self.pending.drain(..) {
            if t < end && events.len() < MAX_EVENTS_PER_CMD {
                let frac = (((t - start) / span).clamp(0.0, 1.0) * 255.0).round() as u8;
                events.push(ButtonEvent {
                    frac,
                    buttons: b,
                    view: v,
                });
            } else {
                rest.push((t, b, v));
            }
        }
        self.pending = rest;
        let view_end = self.view_at(end);
        let cmd = UserCmd {
            tick: 0,
            buttons_start: self.last_buttons,
            view_start: self.last_view,
            view_end,
            events,
            weapon_select: std::mem::take(&mut self.weapon_select),
            view_time: Default::default(),
        };
        self.last_buttons = cmd
            .events
            .last()
            .map(|e| e.buttons)
            .unwrap_or(self.last_buttons);
        self.last_view = view_end;
        self.views.retain(|(t, _)| *t >= end);
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions_keep_their_sub_tick_position_and_view() {
        let mut tl = InputTimeline::default();
        tl.set_view(0.0, ViewAngles::new(0.0, 10.0));
        tl.set_view(0.004, ViewAngles::new(0.0, 20.0));
        tl.set_buttons(0.004, Buttons(Buttons::ATTACK));
        let dt = 1.0 / 128.0;
        let c = tl.build(0.0, dt);
        assert_eq!(c.events.len(), 1);
        let e = c.events[0];
        assert!((e.frac as f64 / 255.0 - 0.004 / dt).abs() < 0.01);
        assert_eq!(e.view.yaw, 20.0);
        assert_eq!(c.buttons_start, Buttons(0));
        let c2 = tl.build(dt, 2.0 * dt);
        assert!(c2.events.is_empty());
        assert_eq!(c2.buttons_start, Buttons(Buttons::ATTACK));
        assert_eq!(c2.view_start.yaw, 20.0);
    }
}
