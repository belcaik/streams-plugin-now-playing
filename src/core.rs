use std::time::Instant;

use crate::model::{Config, Status, Track};

pub trait Source {
    fn snapshot(&mut self) -> Option<Track>;
}

impl<T: Source + ?Sized> Source for Box<T> {
    fn snapshot(&mut self) -> Option<Track> {
        (**self).snapshot()
    }
}

pub struct Candidate {
    pub track: Track,
    pub stable_id: String,
    pub stable_order: usize,
}

pub fn select_candidate(candidates: Vec<Candidate>, config: &Config) -> Option<Track> {
    let mut candidates: Vec<_> = candidates
        .into_iter()
        .filter(|candidate| {
            candidate.track.status != Status::Stopped
                && !config.ignore.contains(&candidate.track.source)
        })
        .collect();
    candidates.sort_by(|left, right| {
        status_rank(left.track.status)
            .cmp(&status_rank(right.track.status))
            .then_with(|| priority(left, config).cmp(&priority(right, config)))
            .then_with(|| left.stable_order.cmp(&right.stable_order))
            .then_with(|| left.stable_id.cmp(&right.stable_id))
    });
    candidates.into_iter().next().map(|mut candidate| {
        candidate.track.clamp_position();
        candidate.track
    })
}

fn status_rank(status: Status) -> u8 {
    match status {
        Status::Playing => 0,
        Status::Paused => 1,
        Status::Stopped => 2,
    }
}

fn priority(candidate: &Candidate, config: &Config) -> usize {
    config
        .priority
        .iter()
        .position(|source| source == &candidate.track.source)
        .unwrap_or(usize::MAX)
}

#[derive(Default)]
pub struct ChangeDetector {
    last: Option<(Track, Instant)>,
}

impl ChangeDetector {
    pub fn new() -> Self {
        Self { last: None }
    }

    pub fn observe(&mut self, next: Track, now: Instant) -> bool {
        let should_emit = self.last.as_ref().is_none_or(|(last, emitted_at)| {
            !same_identity(last, &next) || position_drifted(last, *emitted_at, &next, now)
        });
        if should_emit {
            self.last = Some((next, now));
        }
        should_emit
    }
}

fn same_identity(left: &Track, right: &Track) -> bool {
    left.status == right.status
        && left.source == right.source
        && left.title == right.title
        && left.artist == right.artist
        && left.album == right.album
        && left.art_url == right.art_url
        && left.duration_ms == right.duration_ms
}

fn position_drifted(last: &Track, emitted_at: Instant, next: &Track, now: Instant) -> bool {
    let elapsed_ms = u64::try_from(now.duration_since(emitted_at).as_millis()).unwrap_or(u64::MAX);
    let expected = if last.status == Status::Playing {
        last.position_ms.saturating_add(elapsed_ms)
    } else {
        last.position_ms
    };
    expected.abs_diff(next.position_ms) > 2000
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn candidate(status: Status, source: &str, order: usize, id: &str) -> Candidate {
        Candidate {
            track: Track {
                status,
                title: "Song".into(),
                artist: String::new(),
                album: String::new(),
                art_url: None,
                duration_ms: 10_000,
                position_ms: 0,
                source: source.into(),
            },
            stable_id: id.into(),
            stable_order: order,
        }
    }

    fn config(priority: &[&str], ignore: &[&str]) -> Config {
        Config {
            priority: priority.iter().map(|value| (*value).into()).collect(),
            ignore: ignore.iter().map(|value| (*value).into()).collect(),
            ..Config::default()
        }
    }

    #[test]
    fn playing_beats_paused_regardless_of_priority() {
        let selected = select_candidate(
            vec![
                candidate(Status::Paused, "spotify", 0, "a"),
                candidate(Status::Playing, "other", 1, "b"),
            ],
            &config(&["spotify"], &[]),
        );
        assert_eq!(selected.unwrap().source, "other");
    }

    #[test]
    fn priority_breaks_ties_and_ignore_wins() {
        let selected = select_candidate(
            vec![
                candidate(Status::Playing, "firefox", 0, "a"),
                candidate(Status::Playing, "spotify", 1, "b"),
            ],
            &config(&["spotify", "firefox"], &["spotify"]),
        );
        assert_eq!(selected.unwrap().source, "firefox");
    }

    #[test]
    fn unlisted_sources_follow_listed_sources() {
        let selected = select_candidate(
            vec![
                candidate(Status::Paused, "other", 0, "a"),
                candidate(Status::Paused, "vlc", 1, "b"),
            ],
            &config(&["vlc"], &[]),
        );
        assert_eq!(selected.unwrap().source, "vlc");
    }

    #[test]
    fn discovery_order_then_stable_id_break_remaining_ties() {
        let mut first = candidate(Status::Playing, "same", 1, "a");
        first.track.title = "first".into();
        let mut second = candidate(Status::Playing, "same", 0, "b");
        second.track.title = "second".into();
        let by_order = select_candidate(vec![first, second], &config(&[], &[]));
        assert_eq!(by_order.unwrap().title, "second");

        let mut first = candidate(Status::Playing, "same", 0, "b");
        first.track.title = "first".into();
        let mut second = candidate(Status::Playing, "same", 0, "a");
        second.track.title = "second".into();
        let by_id = select_candidate(vec![first, second], &config(&[], &[]));
        assert_eq!(by_id.unwrap().title, "second");
    }

    #[test]
    fn stopped_or_ignored_candidates_return_none() {
        let selected = select_candidate(
            vec![candidate(Status::Stopped, "spotify", 0, "a")],
            &config(&["spotify"], &[]),
        );
        assert_eq!(selected, None);
    }

    #[test]
    fn selected_track_saturates_position_to_duration() {
        let mut item = candidate(Status::Playing, "spotify", 0, "a");
        item.track.position_ms = 20_000;
        let selected = select_candidate(vec![item], &config(&[], &[])).unwrap();
        assert_eq!(selected.position_ms, 10_000);
    }

    #[test]
    fn detector_emits_first_snapshot_and_every_identity_change() {
        let now = Instant::now();
        let base = candidate(Status::Playing, "spotify", 0, "a").track;
        let mut detector = ChangeDetector::new();
        assert!(detector.observe(base.clone(), now));
        let changes = [
            Track {
                title: "Other".into(),
                ..base.clone()
            },
            Track {
                artist: "Artist".into(),
                ..base.clone()
            },
            Track {
                album: "Album".into(),
                ..base.clone()
            },
            Track {
                art_url: Some("data:image/png;base64,AA==".into()),
                ..base.clone()
            },
            Track {
                duration_ms: 20_000,
                ..base.clone()
            },
            Track {
                source: "vlc".into(),
                ..base.clone()
            },
            Track {
                status: Status::Paused,
                ..base.clone()
            },
        ];
        for changed in changes {
            assert!(detector.observe(changed, now));
        }
    }

    #[test]
    fn detector_ignores_expected_progress_and_exact_two_second_drift() {
        let now = Instant::now();
        let base = candidate(Status::Playing, "spotify", 0, "a").track;
        let mut detector = ChangeDetector::new();
        detector.observe(base.clone(), now);
        assert!(!detector.observe(
            Track {
                position_ms: 1000,
                ..base.clone()
            },
            now + Duration::from_secs(1),
        ));
        assert!(!detector.observe(
            Track {
                position_ms: 3000,
                ..base
            },
            now + Duration::from_secs(1),
        ));
    }

    #[test]
    fn detector_emits_forward_and_backward_seeks() {
        let now = Instant::now();
        let base = candidate(Status::Playing, "spotify", 0, "a").track;
        let mut detector = ChangeDetector::new();
        detector.observe(base.clone(), now);
        assert!(detector.observe(
            Track {
                position_ms: 4000,
                ..base.clone()
            },
            now + Duration::from_secs(1),
        ));
        assert!(detector.observe(
            Track {
                position_ms: 0,
                ..base
            },
            now + Duration::from_secs(2),
        ));
    }

    #[test]
    fn detector_keeps_paused_fixed_and_stopped_only_once() {
        let now = Instant::now();
        let mut detector = ChangeDetector::new();
        let paused = candidate(Status::Paused, "spotify", 0, "a").track;
        detector.observe(paused.clone(), now);
        assert!(!detector.observe(paused, now + Duration::from_secs(10)));
        assert!(detector.observe(Track::stopped(), now));
        assert!(!detector.observe(Track::stopped(), now + Duration::from_secs(1)));
    }
}
