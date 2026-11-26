//! Steadies the mouse battery percentage before it is shown.
//!
//! A mouse works its percentage out from the cell voltage, which dips while the mouse is busy
//! and recovers when it rests, so single readings wander by several points. The shown value is
//! the middle of the last few readings, and it only moves against the expected direction (up
//! while in use, down while charging) once every recent reading agrees.

const WINDOW: usize = 5;

#[derive(Debug, Default)]
pub struct Smoother {
    recent: Vec<u8>,
    charging: bool,
    shown: Option<u8>,
    /// A reading disagreed with the current charging state and is waiting for a second one.
    flip_pending: bool,
}

impl Smoother {
    /// Take one raw reading and return the percentage and charging state to show.
    pub fn push(&mut self, percent: u8, charging: bool) -> (u8, bool) {
        let percent = percent.min(100);
        if charging != self.charging {
            // Plugging in or unplugging must be seen twice in a row before it is believed.
            if let (Some(shown), false) = (self.shown, self.flip_pending) {
                self.flip_pending = true;
                return (shown, self.charging);
            }
            // It starts a new run; old readings no longer apply.
            self.charging = charging;
            self.recent.clear();
            self.shown = None;
        }
        self.flip_pending = false;
        if self.recent.len() == WINDOW {
            self.recent.remove(0);
        }
        self.recent.push(percent);
        let mut sorted = self.recent.clone();
        sorted.sort_unstable();
        let middle = sorted[(sorted.len() - 1) / 2];
        let (lowest, highest) = (sorted[0], sorted[sorted.len() - 1]);
        let full = self.recent.len() == WINDOW;
        let next = match self.shown {
            None => middle,
            Some(shown) if charging => {
                if middle > shown {
                    middle
                } else if full && highest < shown {
                    highest
                } else {
                    shown
                }
            }
            Some(shown) => {
                if middle < shown {
                    middle
                } else if full && lowest > shown {
                    lowest
                } else {
                    shown
                }
            }
        };
        self.shown = Some(next);
        (next, charging)
    }
}

/// The charge as one of a few bands. The mouse's own percentage wanders by a few points, so the
/// app shows the band instead of the number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Urgent,
    Low,
    Mid,
    Good,
}

impl Tier {
    pub fn id(self) -> &'static str {
        match self {
            Tier::Urgent => "urgent",
            Tier::Low => "low",
            Tier::Mid => "mid",
            Tier::Good => "good",
        }
    }

    /// The word shown on the taskbar strip.
    pub fn label(self) -> &'static str {
        match self {
            Tier::Urgent => "Charge now",
            Tier::Low => "Low",
            Tier::Mid => "Mid",
            Tier::Good => "Good",
        }
    }
}

/// Lowest percentage of Low, Mid and Good; below the first is Urgent.
const FLOORS: [(u8, Tier); 3] = [(8, Tier::Low), (26, Tier::Mid), (51, Tier::Good)];
/// A better band is only entered once the charge is this far past its floor, so a reading
/// that hovers on a boundary does not flip the band back and forth.
const CLIMB: u8 = 4;

/// The band for a percentage, given the band shown before.
pub fn tier(percent: u8, previous: Option<Tier>) -> Tier {
    let band = |margin: u8| FLOORS.iter().rev().find(|(floor, _)| percent >= floor + margin).map_or(Tier::Urgent, |f| f.1);
    let plain = band(0);
    match previous {
        Some(before) if plain > before => band(CLIMB).max(before),
        _ => plain,
    }
}

/// How many checks in a row must agree before Low or Charge now is shown.
const CONFIRM: u8 = 3;

/// The band on screen. Good and Mid show at once. Low and Charge now set off colours and a
/// notification, so they are held back until several checks in a row say the same; a single
/// wrong reading, or the first reading after the mouse wakes, can never raise an alarm.
#[derive(Debug, Default)]
pub struct Bands {
    shown: Option<Tier>,
    /// The mildest band seen in the current run of alarming checks, and how long the run is.
    doubt: Option<(Tier, u8)>,
}

impl Bands {
    pub fn shown(&self) -> Option<Tier> {
        self.shown
    }

    /// Take one steadied percentage and return the band to show, if any yet.
    pub fn push(&mut self, percent: u8) -> Option<Tier> {
        let next = tier(percent, self.shown);
        let alarming = next <= Tier::Low && self.shown.map_or(true, |shown| next < shown);
        if !alarming {
            self.doubt = None;
            self.shown = Some(next);
            return self.shown;
        }
        let (mildest, run) = match self.doubt {
            Some((mildest, run)) => (mildest.max(next), run + 1),
            None => (next, 1),
        };
        if run >= CONFIRM {
            self.doubt = None;
            self.shown = Some(mildest);
        } else {
            self.doubt = Some((mildest, run));
        }
        self.shown
    }
}

#[cfg(test)]
mod tests {
    use super::Smoother;

    fn run(readings: &[u8], charging: bool) -> Vec<u8> {
        let mut s = Smoother::default();
        readings.iter().map(|&p| s.push(p, charging).0).collect()
    }

    #[test]
    fn a_single_odd_reading_is_ignored() {
        assert_eq!(run(&[59, 59, 59, 45, 59, 59], false), [59, 59, 59, 59, 59, 59]);
        assert_eq!(run(&[59, 59, 59, 72, 59, 59], false), [59, 59, 59, 59, 59, 59]);
    }

    #[test]
    fn a_real_drop_is_followed_within_a_few_readings() {
        assert_eq!(run(&[59, 59, 59, 45, 45, 45], false), [59, 59, 59, 59, 59, 45]);
    }

    #[test]
    fn it_does_not_creep_up_while_in_use_unless_every_reading_agrees() {
        assert_eq!(run(&[50, 50, 50, 52, 51, 52, 50, 52], false), [50; 8]);
        assert_eq!(run(&[50, 55, 55, 55, 55, 55], false), [50, 50, 50, 50, 50, 55]);
    }

    #[test]
    fn charging_rises_and_resets_when_unplugged() {
        let mut s = Smoother::default();
        assert_eq!(s.push(40, true), (40, true));
        assert_eq!(s.push(39, true), (40, true));
        assert_eq!(s.push(42, true), (40, true));
        assert_eq!(s.push(42, true), (40, true));
        assert_eq!(s.push(43, true), (42, true));
        assert_eq!(s.push(38, false), (42, true));
        assert_eq!(s.push(38, false), (38, false));
    }

    #[test]
    fn one_stray_charging_reading_changes_nothing() {
        let mut s = Smoother::default();
        assert_eq!(s.push(60, false), (60, false));
        assert_eq!(s.push(15, true), (60, false));
        assert_eq!(s.push(60, false), (60, false));
        assert_eq!(s.push(1, true), (60, false));
        assert_eq!(s.push(60, false), (60, false));
    }

    #[test]
    fn bands_cover_the_range() {
        use super::{tier, Tier};
        assert_eq!(tier(100, None), Tier::Good);
        assert_eq!(tier(51, None), Tier::Good);
        assert_eq!(tier(50, None), Tier::Mid);
        assert_eq!(tier(26, None), Tier::Mid);
        assert_eq!(tier(25, None), Tier::Low);
        assert_eq!(tier(20, None), Tier::Low);
        assert_eq!(tier(8, None), Tier::Low);
        assert_eq!(tier(7, None), Tier::Urgent);
        assert_eq!(tier(0, None), Tier::Urgent);
    }

    #[test]
    fn a_band_does_not_flicker_at_its_edge() {
        use super::{tier, Tier};
        // Dropping is immediate, climbing back needs a clear margin.
        assert_eq!(tier(50, Some(Tier::Good)), Tier::Mid);
        assert_eq!(tier(52, Some(Tier::Mid)), Tier::Mid);
        assert_eq!(tier(54, Some(Tier::Mid)), Tier::Mid);
        assert_eq!(tier(55, Some(Tier::Mid)), Tier::Good);
        assert_eq!(tier(9, Some(Tier::Urgent)), Tier::Urgent);
        assert_eq!(tier(12, Some(Tier::Urgent)), Tier::Low);
        // A big jump while charging lands in the right band at once.
        assert_eq!(tier(90, Some(Tier::Urgent)), Tier::Good);
        assert_eq!(tier(22, Some(Tier::Urgent)), Tier::Low);
    }

    #[test]
    fn an_alarm_needs_several_checks_in_a_row() {
        use super::{Bands, Tier};
        let mut b = Bands::default();
        assert_eq!(b.push(60), Some(Tier::Good));
        // One or two wrong readings change nothing.
        assert_eq!(b.push(1), Some(Tier::Good));
        assert_eq!(b.push(1), Some(Tier::Good));
        assert_eq!(b.push(60), Some(Tier::Good));
        assert_eq!(b.push(15), Some(Tier::Good));
        assert_eq!(b.push(60), Some(Tier::Good));
        // Three in a row are believed, at the mildest level they agree on.
        assert_eq!(b.push(5), Some(Tier::Good));
        assert_eq!(b.push(15), Some(Tier::Good));
        assert_eq!(b.push(5), Some(Tier::Low));
        assert_eq!(b.push(5), Some(Tier::Low));
        assert_eq!(b.push(5), Some(Tier::Low));
        assert_eq!(b.push(5), Some(Tier::Urgent));
        // Good news is shown at once.
        assert_eq!(b.push(80), Some(Tier::Good));
    }

    #[test]
    fn nothing_alarming_is_shown_on_the_first_reading() {
        use super::{Bands, Tier};
        let mut b = Bands::default();
        assert_eq!(b.push(3), None);
        assert_eq!(b.push(3), None);
        assert_eq!(b.push(3), Some(Tier::Urgent));
        let mut b = Bands::default();
        assert_eq!(b.push(3), None);
        assert_eq!(b.push(40), Some(Tier::Mid));
    }
}
