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

#[cfg(test)]
mod tests {
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
}
