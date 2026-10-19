use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use uuid::Uuid;

pub const SLOTS: usize = 4;
pub const BUTTON_NAMES: [&str; 15] = [
    "A", "B", "X", "Y", "View", "Guide", "Menu", "L3", "R3", "LB", "RB", "Up", "Down", "Left",
    "Right",
];
pub const VALID_BUTTONS: u32 = (1 << BUTTON_NAMES.len()) - 1;

/// SDL convention: negative Y points up. Triggers retain their full 15-bit range.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PadState {
    pub buttons: u32,
    pub axes: [i16; 4],
    pub triggers: [u16; 2],
}
impl PadState {
    pub fn valid(&self) -> bool {
        self.buttons & !VALID_BUTTONS == 0 && self.triggers.iter().all(|v| *v <= 32767)
    }
    pub fn neutral(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PadInfo {
    pub id: u32,
    pub name: String,
    pub vendor: u16,
    pub product: u16,
    pub connection: String,
    pub power: String,
    pub battery_percent: Option<i32>,
    pub rumble: bool,
    pub trigger_rumble: bool,
    pub state: PadState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AxisSettings {
    pub dead_zone: f32,
    pub curve: f32,
    pub sensitivity: f32,
}
impl Default for AxisSettings {
    fn default() -> Self {
        Self {
            dead_zone: 0.08,
            curve: 1.0,
            sensitivity: 1.0,
        }
    }
}
impl AxisSettings {
    pub fn valid(&self) -> bool {
        self.dead_zone.is_finite()
            && (0.0..=0.5).contains(&self.dead_zone)
            && self.curve.is_finite()
            && (0.25..=4.0).contains(&self.curve)
            && self.sensitivity.is_finite()
            && (0.1..=3.0).contains(&self.sensitivity)
    }
    pub fn apply(&self, x: i16, y: i16) -> (i16, i16) {
        if !self.valid() {
            return (0, 0);
        }
        let (fx, fy) = (x as f32 / 32768.0, y as f32 / 32768.0);
        let radius = fx.hypot(fy);
        if radius <= self.dead_zone {
            return (0, 0);
        }
        let magnitude = (((radius.min(1.0) - self.dead_zone) / (1.0 - self.dead_zone))
            .powf(self.curve)
            * self.sensitivity)
            .min(1.0);
        let axis = |v: f32| {
            (v / radius * magnitude * 32767.0)
                .round()
                .clamp(-32768.0, 32767.0) as i16
        };
        (axis(fx), axis(fy))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSettings {
    pub sticks: AxisSettings,
    pub mapping: [u8; 15],
}
impl Default for InputSettings {
    fn default() -> Self {
        Self {
            sticks: AxisSettings::default(),
            mapping: std::array::from_fn(|i| i as u8),
        }
    }
}
impl InputSettings {
    pub fn valid(&self) -> bool {
        self.sticks.valid() && self.mapping.iter().all(|v| *v < 15)
    }
    pub fn apply(&self, mut state: PadState) -> PadState {
        if !self.valid() {
            return PadState::default();
        }
        let buttons = state.buttons;
        state.buttons = 0;
        for (from, to) in self.mapping.iter().enumerate() {
            if buttons & (1 << from) != 0 {
                state.buttons |= 1 << to;
            }
        }
        (state.axes[0], state.axes[1]) = self.sticks.apply(state.axes[0], state.axes[1]);
        (state.axes[2], state.axes[3]) = self.sticks.apply(state.axes[2], state.axes[3]);
        state
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owner {
    pub peer: Uuid,
    pub controller: u32,
}
#[derive(Default)]
pub struct Slots {
    owners: [Option<Owner>; SLOTS],
}
impl Slots {
    pub fn assign(&mut self, slot: u8, owner: Owner) -> Result<(), &'static str> {
        if slot as usize >= SLOTS {
            return Err("slot must be 0..3");
        }
        if self.owners[slot as usize].is_some_and(|current| current != owner) {
            return Err("slot occupied");
        }
        if self
            .owners
            .iter()
            .enumerate()
            .any(|(i, o)| i != slot as usize && *o == Some(owner))
        {
            return Err("controller already assigned");
        }
        self.owners[slot as usize] = Some(owner);
        Ok(())
    }
    pub fn permits(&self, slot: u8, owner: Owner) -> bool {
        self.owners.get(slot as usize) == Some(&Some(owner))
    }
    pub fn revoke(&mut self, owner: Owner) {
        for value in &mut self.owners {
            if *value == Some(owner) {
                *value = None;
            }
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Percentiles {
    pub samples: usize,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
}
pub fn percentiles(values: &[f64]) -> Percentiles {
    let mut values: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if values.is_empty() {
        return Percentiles::default();
    }
    values.sort_by(f64::total_cmp);
    let at = |p: f64| {
        values[((values.len() as f64 * p).ceil() as usize)
            .saturating_sub(1)
            .min(values.len() - 1)]
    };
    Percentiles {
        samples: values.len(),
        p50_ms: at(0.5),
        p95_ms: at(0.95),
        p99_ms: at(0.99),
    }
}

/// A heartbeat cannot preserve a stale input state: only fresh accepted reports refresh this timer.
pub struct Watchdog {
    last: Option<Instant>,
    timeout: Duration,
}
impl Watchdog {
    pub fn new(timeout: Duration) -> Self {
        Self {
            last: None,
            timeout,
        }
    }
    pub fn accepted(&mut self, now: Instant) {
        self.last = Some(now);
    }
    pub fn expired(&mut self, now: Instant) -> bool {
        if self
            .last
            .is_some_and(|last| now.saturating_duration_since(last) >= self.timeout)
        {
            self.last = None;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn analog_is_analog() {
        let c = AxisSettings {
            dead_zone: 0.0,
            ..Default::default()
        };
        for v in [6553, 16384, 32767] {
            let (x, y) = c.apply(v, 0);
            assert!((x - v).abs() <= 1);
            assert_eq!(y, 0);
        }
        assert_eq!(AxisSettings::default().apply(100, 100), (0, 0));
        assert!(c.apply(-32768, 0).0 < -32760);
    }
    #[test]
    fn slots_dont_override() {
        let mut s = Slots::default();
        let a = Owner {
            peer: Uuid::new_v4(),
            controller: 1,
        };
        let b = Owner {
            peer: Uuid::new_v4(),
            controller: 1,
        };
        s.assign(0, a).unwrap();
        s.assign(1, b).unwrap();
        assert!(s.assign(0, b).is_err());
        assert!(!s.permits(0, b));
        s.revoke(b);
        assert!(!s.permits(1, b));
        assert!(s.permits(0, a));
    }
    #[test]
    fn watchdog_releases_once() {
        let now = Instant::now();
        let mut w = Watchdog::new(Duration::from_millis(250));
        w.accepted(now);
        assert!(!w.expired(now + Duration::from_millis(249)));
        assert!(w.expired(now + Duration::from_millis(250)));
        assert!(!w.expired(now + Duration::from_secs(5)));
    }
    #[test]
    fn curves_validate_and_map_multiple_buttons() {
        let mut c = InputSettings::default();
        c.mapping.swap(0, 1);
        let v = c.apply(PadState {
            buttons: 1 | 4,
            ..Default::default()
        });
        assert_eq!(v.buttons, 2 | 4);
        c.sticks.dead_zone = f32::NAN;
        assert!(!c.valid());
        assert!(c.apply(v).neutral());
    }
}
