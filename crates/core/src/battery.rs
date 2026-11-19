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
}

impl Smoother {
    /// Take one raw reading and return the percentage to show.
    pub fn push(&mut self, percent: u8, charging: bool) -> u8 {
        let percent = percent.min(100);
        if charging != self.charging {
            // Plugging in or unplugging starts a new run; old readings no longer apply.
            self.charging = charging;
            self.recent.clear();
            self.shown = None;
        }
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
        next
    }
}

#[cfg(test)]
mod tests {
    use super::Smoother;

    fn run(readings: &[u8], charging: bool) -> Vec<u8> {
        let mut s = Smoother::default();
        readings.iter().map(|&p| s.push(p, charging)).collect()
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
        assert_eq!(s.push(40, true), 40);
        assert_eq!(s.push(39, true), 40);
        assert_eq!(s.push(42, true), 40);
        assert_eq!(s.push(42, true), 40);
        assert_eq!(s.push(43, true), 42);
        assert_eq!(s.push(38, false), 38);
    }
}
