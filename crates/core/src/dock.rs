//! Where the weather docked on the taskbar goes, from what the taskbar says about its buttons.
//!
//! The taskbar is asked now and then where its Start button is. Its answers are not always
//! sound: while it is busy, hidden or rearranging its buttons it can give none at all, or a
//! button without a place. The strip must not jump about, change its look or vanish on such an
//! answer, so it follows only what the taskbar says consistently:
//!
//! * no answer (or one that makes no sense) changes nothing;
//! * the first sound answer counts at once, as there is nothing to keep yet;
//! * an answer for the same place counts at once, so the strip gets narrower straight away when
//!   app buttons take some of its room and never sits on them;
//! * any other answer (another place, no room, no free stretch) counts only once the taskbar has
//!   given it twice in a row.

/// What the taskbar's answer means for the strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// A free stretch left of Start: where the strip starts (physical px) and how wide it may be
    /// (logical px).
    Dock { x: i32, room: u32 },
    /// A centered taskbar that has filled up: the strip steps aside until room returns.
    Crowded,
    /// No free stretch at all (the buttons start at the left): the strip sits by the tray.
    Beside,
}

impl Answer {
    /// The same place and look; only the room may differ.
    pub fn same_place(self, other: Answer) -> bool {
        match (self, other) {
            (Answer::Dock { x: a, .. }, Answer::Dock { x: b, .. }) => a == b,
            (Answer::Crowded, Answer::Crowded) | (Answer::Beside, Answer::Beside) => true,
            _ => false,
        }
    }
}

/// What the strip follows, and an answer that is waiting to be given again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Memory {
    pub current: Option<Answer>,
    pub pending: Option<(Answer, u8)>,
    /// Readings in a row that gave no usable answer.
    pub unusable: u32,
}

impl Memory {
    /// Take in one reading (`None`: no usable answer) and return what the strip follows now.
    pub fn follow(&mut self, reading: Option<Answer>) -> Option<Answer> {
        let Some(answer) = reading else {
            self.unusable = self.unusable.saturating_add(1);
            return self.current;
        };
        self.unusable = 0;
        match self.current {
            Some(current) if !current.same_place(answer) => {
                let times = match self.pending {
                    Some((waiting, n)) if waiting.same_place(answer) => n + 1,
                    _ => 1,
                };
                if times >= 2 {
                    self.current = Some(answer);
                    self.pending = None;
                } else {
                    self.pending = Some((answer, times));
                }
            }
            _ => {
                self.current = Some(answer);
                self.pending = None;
            }
        }
        self.current
    }

    /// How many half-second ticks until the taskbar should be asked again, when that should be
    /// sooner than usual: a second after an answer that waits to be given again, and soon (then
    /// less and less often) while the taskbar has not given any usable answer yet.
    pub fn ask_again_in(&self) -> Option<u64> {
        if self.pending.is_some() {
            Some(2)
        } else if self.current.is_none() {
            Some((2u64 << self.unusable.min(5)).min(60))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: Answer = Answer::Dock { x: 12, room: 900 };

    #[test]
    fn the_first_sound_answer_counts_at_once() {
        let mut m = Memory::default();
        assert_eq!(m.follow(Some(HOME)), Some(HOME));
        let mut m = Memory::default();
        assert_eq!(m.follow(Some(Answer::Crowded)), Some(Answer::Crowded));
    }

    #[test]
    fn no_answer_changes_nothing() {
        let mut m = Memory::default();
        assert_eq!(m.follow(None), None);
        m.follow(Some(HOME));
        for _ in 0..20 {
            assert_eq!(m.follow(None), Some(HOME));
        }
        assert_eq!(m.unusable, 20);
        assert_eq!(m.follow(Some(HOME)), Some(HOME));
        assert_eq!(m.unusable, 0);
    }

    #[test]
    fn less_room_in_the_same_place_counts_at_once() {
        let mut m = Memory::default();
        m.follow(Some(HOME));
        let narrower = Answer::Dock { x: 12, room: 219 };
        assert_eq!(m.follow(Some(narrower)), Some(narrower));
        assert_eq!(m.pending, None);
    }

    #[test]
    fn stepping_aside_or_moving_needs_the_same_answer_twice() {
        for other in [Answer::Crowded, Answer::Beside, Answer::Dock { x: 300, room: 600 }] {
            let mut m = Memory::default();
            m.follow(Some(HOME));
            assert_eq!(m.follow(Some(other)), Some(HOME), "{other:?} once");
            assert_eq!(m.ask_again_in(), Some(2));
            assert_eq!(m.follow(Some(other)), Some(other), "{other:?} twice");
            assert_eq!(m.ask_again_in(), None);
        }
    }

    #[test]
    fn a_lone_odd_answer_is_forgotten() {
        let mut m = Memory::default();
        m.follow(Some(HOME));
        m.follow(Some(Answer::Crowded));
        m.follow(Some(HOME));
        assert_eq!(m.pending, None);
        assert_eq!(m.follow(Some(Answer::Crowded)), Some(HOME));
        // Two different odd answers in a row are not the same answer twice.
        assert_eq!(m.follow(Some(Answer::Beside)), Some(HOME));
        assert_eq!(m.follow(Some(Answer::Beside)), Some(Answer::Beside));
    }

    #[test]
    fn no_answer_in_between_still_counts_as_in_a_row() {
        let mut m = Memory::default();
        m.follow(Some(HOME));
        m.follow(Some(Answer::Crowded));
        m.follow(None);
        assert_eq!(m.follow(Some(Answer::Crowded)), Some(Answer::Crowded));
    }

    #[test]
    fn a_taskbar_that_does_not_answer_is_asked_less_and_less_often() {
        let mut m = Memory::default();
        let waits: Vec<u64> = (0..8).map(|_| { m.follow(None); m.ask_again_in().unwrap() }).collect();
        assert_eq!(waits, vec![4, 8, 16, 32, 60, 60, 60, 60]);
        m.follow(Some(HOME));
        assert_eq!(m.ask_again_in(), None);
    }
}
