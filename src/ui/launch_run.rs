//! Progress of one launch as the dashboard shows it: a status per zone of the launched view.
use super::theme::Tone;
use crate::core::launcher::{Outcome, Report};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub enum ZoneState {
    Pending,
    Arranged,
    Opened(Duration),
    Failed(String),
}

pub struct LaunchRun {
    pub view: usize,
    /// For each launched zone, its index in the view; a retry launches only some zones.
    launched: Vec<usize>,
    /// State per zone index of the view; `None` for zones this run did not launch.
    pub states: Vec<Option<ZoneState>>,
    started: Instant,
    pub finished: Option<Duration>,
}

impl LaunchRun {
    pub fn new(view: usize, zone_count: usize, launched: Vec<usize>) -> Self {
        let mut states = vec![None; zone_count];
        for &index in &launched {
            if let Some(state) = states.get_mut(index) {
                *state = Some(ZoneState::Pending);
            }
        }
        Self {
            view,
            launched,
            states,
            started: Instant::now(),
            finished: None,
        }
    }

    pub fn apply(&mut self, report: &Report) {
        let elapsed = self.started.elapsed();
        let state = match &report.outcome {
            Outcome::Arranged => ZoneState::Arranged,
            Outcome::Opened => ZoneState::Opened(elapsed),
            Outcome::Failed(error) => ZoneState::Failed(error.clone()),
        };
        match report.zone {
            Some(zone) => {
                if let Some(slot) = self
                    .launched
                    .get(zone)
                    .and_then(|index| self.states.get_mut(*index))
                {
                    *slot = Some(state);
                }
            }
            // A view-wide failure ends every zone that is still waiting.
            None => {
                for slot in self.states.iter_mut().flatten() {
                    if *slot == ZoneState::Pending {
                        *slot = state.clone();
                    }
                }
            }
        }
        if self.finished.is_none() && self.pending() == 0 {
            self.finished = Some(elapsed);
        }
    }

    pub fn pending(&self) -> usize {
        self.states
            .iter()
            .flatten()
            .filter(|state| **state == ZoneState::Pending)
            .count()
    }

    pub fn placed(&self) -> usize {
        self.states
            .iter()
            .flatten()
            .filter(|state| matches!(state, ZoneState::Arranged | ZoneState::Opened(_)))
            .count()
    }

    pub fn total(&self) -> usize {
        self.states.iter().flatten().count()
    }

    pub fn failed(&self) -> Vec<usize> {
        self.states
            .iter()
            .enumerate()
            .filter(|(_, state)| matches!(state, Some(ZoneState::Failed(_))))
            .map(|(index, _)| index)
            .collect()
    }

    pub fn elapsed(&self) -> Duration {
        self.finished.unwrap_or_else(|| self.started.elapsed())
    }

    pub fn age(&self) -> Duration {
        self.started.elapsed()
    }

    /// Card label for the canvas; finished successful zones show nothing.
    pub fn badge(&self, index: usize) -> Option<(String, Tone)> {
        match self.states.get(index)?.as_ref()? {
            ZoneState::Pending => Some(("opening…".into(), Tone::Muted)),
            ZoneState::Failed(_) => Some(("failed".into(), Tone::Danger)),
            _ if self.finished.is_none() => Some(("placed".into(), Tone::Ok)),
            _ => None,
        }
    }
}

pub fn describe(state: &ZoneState) -> String {
    match state {
        ZoneState::Pending => "opening…".into(),
        ZoneState::Arranged => "arranged".into(),
        ZoneState::Opened(after) => format!("opened · {:.2} s", after.as_secs_f32()),
        ZoneState::Failed(error) => error.clone(),
    }
}

/// "just now", "3 min ago", "2 h ago".
pub fn ago(age: Duration) -> String {
    match age.as_secs() {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", age.as_secs() / 60),
        _ => format!("{} h ago", age.as_secs() / 3600),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(zone: Option<usize>, outcome: Outcome) -> Report {
        Report {
            zone,
            name: String::new(),
            outcome,
        }
    }

    #[test]
    fn tracks_zones_until_every_launched_zone_reported() {
        let mut run = LaunchRun::new(0, 3, vec![0, 1, 2]);
        assert_eq!((run.pending(), run.total()), (3, 3));
        run.apply(&report(Some(0), Outcome::Arranged));
        run.apply(&report(Some(2), Outcome::Failed("No window".into())));
        assert_eq!(run.badge(0), Some(("placed".into(), Tone::Ok)));
        assert_eq!(run.badge(1), Some(("opening…".into(), Tone::Muted)));
        assert!(run.finished.is_none());
        run.apply(&report(Some(1), Outcome::Opened));
        assert!(run.finished.is_some());
        assert_eq!((run.placed(), run.failed()), (2, vec![2]));
        assert_eq!(run.badge(0), None);
        assert_eq!(run.badge(2), Some(("failed".into(), Tone::Danger)));
    }

    #[test]
    fn retry_maps_reports_back_to_view_zones() {
        let mut run = LaunchRun::new(4, 5, vec![3]);
        assert_eq!(run.total(), 1);
        assert_eq!(run.badge(0), None);
        run.apply(&report(Some(0), Outcome::Opened));
        assert!(matches!(run.states[3], Some(ZoneState::Opened(_))));
        assert_eq!(run.placed(), 1);
    }

    #[test]
    fn view_failure_ends_waiting_zones_only() {
        let mut run = LaunchRun::new(0, 2, vec![0, 1]);
        run.apply(&report(Some(0), Outcome::Arranged));
        run.apply(&report(
            None,
            Outcome::Failed("GNOME bridge unavailable".into()),
        ));
        assert_eq!(run.states[0], Some(ZoneState::Arranged));
        assert_eq!(
            run.states[1],
            Some(ZoneState::Failed("GNOME bridge unavailable".into()))
        );
        assert!(run.finished.is_some());
        assert_eq!(ago(Duration::from_secs(150)), "2 min ago");
    }
}
