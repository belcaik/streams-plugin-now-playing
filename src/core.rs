use crate::{
    model::{Config, Status, Track},
    protocol::{Init, read_init, write_event},
};
use std::{
    io::{self, BufRead, Write},
    sync::mpsc::{Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant},
};

pub trait Source {
    fn snapshot(&mut self) -> Option<Track>;
}
impl<T: Source + ?Sized> Source for Box<T> {
    fn snapshot(&mut self) -> Option<Track> {
        (**self).snapshot()
    }
}
#[derive(Clone)]
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
        rank(left.track.status)
            .cmp(&rank(right.track.status))
            .then_with(|| priority(left, config).cmp(&priority(right, config)))
            .then_with(|| left.stable_order.cmp(&right.stable_order))
            .then_with(|| left.stable_id.cmp(&right.stable_id))
    });
    candidates.into_iter().next().map(|mut candidate| {
        candidate.track.clamp_position();
        candidate.track
    })
}
fn rank(status: Status) -> u8 {
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
    pub fn observe(&mut self, next: Track, now: Instant) -> bool {
        let emit = self.last.as_ref().is_none_or(|(last, at)| {
            !same_identity(last, &next) || position_drifted(last, *at, &next, now)
        });
        if emit {
            self.last = Some((next, now));
        }
        emit
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
fn position_drifted(last: &Track, at: Instant, next: &Track, now: Instant) -> bool {
    let elapsed = u64::try_from(now.duration_since(at).as_millis()).unwrap_or(u64::MAX);
    let expected = if last.status == Status::Playing {
        last.position_ms.saturating_add(elapsed)
    } else {
        last.position_ms
    };
    expected.abs_diff(next.position_ms) > 2000
}

pub trait Clock {
    fn now(&self) -> Instant;
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}
pub enum WorkerMessage {
    Init(Result<Init, String>),
    Tick,
    Eof,
}

pub fn run_worker<R, W, S, C, F>(
    reader: R,
    writer: &mut W,
    factory: F,
    clock: C,
    channel: (Sender<WorkerMessage>, Receiver<WorkerMessage>),
) -> io::Result<()>
where
    R: BufRead + Send + 'static,
    W: Write,
    S: Source,
    C: Clock,
    F: FnOnce(&Config) -> io::Result<S>,
{
    let (tx, rx) = channel;
    thread::spawn(move || input_loop(reader, tx));
    let init = match rx.recv() {
        Ok(WorkerMessage::Init(Ok(init))) => init,
        Ok(WorkerMessage::Init(Err(error))) => return Err(io::Error::other(error)),
        _ => return Ok(()),
    };
    let timeout = Duration::from_millis(init.config.poll_ms);
    let mut source = factory(&init.config)?;
    run_loop(&mut source, writer, clock, rx, timeout)
}
fn run_loop<S: Source, W: Write, C: Clock>(
    source: &mut S,
    writer: &mut W,
    clock: C,
    rx: Receiver<WorkerMessage>,
    timeout: Duration,
) -> io::Result<()> {
    let mut detector = ChangeDetector::default();
    loop {
        let track = source.snapshot().unwrap_or_else(Track::stopped);
        if detector.observe(track.clone(), clock.now()) {
            write_event(writer, &track).map_err(|error| io::Error::other(error.to_string()))?;
        }
        match rx.recv_timeout(timeout) {
            Ok(WorkerMessage::Tick) | Err(RecvTimeoutError::Timeout) => {}
            Ok(WorkerMessage::Eof) | Err(RecvTimeoutError::Disconnected) => return Ok(()),
            Ok(WorkerMessage::Init(_)) => eprintln!("evento init inesperado descartado"),
        }
    }
}
fn input_loop(mut reader: impl BufRead, tx: Sender<WorkerMessage>) {
    let init = read_init(&mut reader).map_err(|error| error.to_string());
    let valid = init.is_ok();
    if tx.send(WorkerMessage::Init(init)).is_err() || !valid {
        return;
    }
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => eprintln!("evento de entrada inesperado descartado"),
            Err(error) => {
                eprintln!("error leyendo stdin: {error}");
                break;
            }
        }
    }
    let _ = tx.send(WorkerMessage::Eof);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, io::Cursor};
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
            priority: priority.iter().map(|v| (*v).into()).collect(),
            ignore: ignore.iter().map(|v| (*v).into()).collect(),
            ..Config::default()
        }
    }
    #[test]
    fn selection_prioritizes_status_sources_and_ignore() {
        let items = vec![
            candidate(Status::Paused, "spotify", 0, "a"),
            candidate(Status::Playing, "other", 1, "b"),
        ];
        assert_eq!(
            select_candidate(items, &config(&["spotify"], &[]))
                .unwrap()
                .source,
            "other"
        );
        let items = vec![
            candidate(Status::Playing, "firefox", 0, "a"),
            candidate(Status::Playing, "spotify", 1, "b"),
        ];
        assert_eq!(
            select_candidate(items, &config(&["spotify"], &["spotify"]))
                .unwrap()
                .source,
            "firefox"
        );
        let items = vec![
            candidate(Status::Paused, "other", 0, "a"),
            candidate(Status::Paused, "vlc", 1, "b"),
        ];
        assert_eq!(
            select_candidate(items, &config(&["vlc"], &[]))
                .unwrap()
                .source,
            "vlc"
        );
        assert_eq!(
            select_candidate(
                vec![candidate(Status::Stopped, "s", 0, "a")],
                &config(&[], &[])
            ),
            None
        );
        let mut a = candidate(Status::Playing, "same", 1, "a");
        let mut b = candidate(Status::Playing, "same", 0, "b");
        a.track.title = "a".into();
        b.track.title = "b".into();
        assert_eq!(
            select_candidate(vec![a.clone(), b.clone()], &config(&[], &[]))
                .unwrap()
                .title,
            "b"
        );
        a.stable_order = 0;
        b.stable_order = 0;
        assert_eq!(
            select_candidate(vec![b, a], &config(&[], &[]))
                .unwrap()
                .title,
            "a"
        );
    }
    #[test]
    fn detector_emits_first_snapshot_and_identity_changes() {
        let now = Instant::now();
        let base = candidate(Status::Playing, "s", 0, "a").track;
        let mut detector = ChangeDetector::default();
        assert!(detector.observe(base.clone(), now));
        let mut changed = base.clone();
        changed.title = "x".into();
        assert!(detector.observe(changed.clone(), now));
        changed.artist = "x".into();
        assert!(detector.observe(changed.clone(), now));
        changed.album = "x".into();
        assert!(detector.observe(changed.clone(), now));
        changed.art_url = Some("data:".into());
        assert!(detector.observe(changed.clone(), now));
        changed.duration_ms = 20_000;
        assert!(detector.observe(changed.clone(), now));
        changed.source = "x".into();
        assert!(detector.observe(changed.clone(), now));
        changed.status = Status::Paused;
        assert!(detector.observe(changed, now));
        let now = Instant::now();
        let base = candidate(Status::Playing, "s", 0, "a").track;
        let mut detector = ChangeDetector::default();
        detector.observe(base.clone(), now);
        for (position, elapsed, expected) in [
            (1000, 1, false),
            (3000, 1, false),
            (4000, 1, true),
            (0, 2, true),
        ] {
            assert_eq!(
                detector.observe(
                    Track {
                        position_ms: position,
                        ..base.clone()
                    },
                    now + Duration::from_secs(elapsed)
                ),
                expected
            );
        }
        let paused = Track {
            status: Status::Paused,
            ..base
        };
        detector.observe(paused.clone(), now);
        assert!(!detector.observe(paused, now + Duration::from_secs(10)));
        assert!(detector.observe(Track::stopped(), now));
        assert!(!detector.observe(Track::stopped(), now + Duration::from_secs(1)));
    }
    struct FakeSource(VecDeque<Track>);
    impl Source for FakeSource {
        fn snapshot(&mut self) -> Option<Track> {
            self.0.pop_front()
        }
    }
    const INIT: &[u8] = br#"{"type":"init","config":{},"data_dir":"."}"#;
    #[test]
    fn worker_emits_exact_lines_and_not_one_per_tick() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(WorkerMessage::Tick).unwrap();
        tx.send(WorkerMessage::Tick).unwrap();
        tx.send(WorkerMessage::Eof).unwrap();
        let base = candidate(Status::Playing, "s", 0, "a").track;
        let mut source = FakeSource(VecDeque::from([
            base.clone(),
            Track {
                position_ms: 1000,
                ..base
            },
            Track::stopped(),
        ]));
        let mut output = Vec::new();
        run_loop(
            &mut source,
            &mut output,
            SystemClock,
            rx,
            Duration::from_secs(60),
        )
        .unwrap();
        let output = String::from_utf8(output).unwrap();
        let expected = "{\"type\":\"now_playing\",\"data\":{\"status\":\"playing\",\"title\":\"Song\",\"artist\":\"\",\"album\":\"\",\"art_url\":null,\"duration_ms\":10000,\"position_ms\":0,\"source\":\"s\"}}\n{\"type\":\"now_playing\",\"data\":{\"status\":\"stopped\",\"title\":\"\",\"artist\":\"\",\"album\":\"\",\"art_url\":null,\"duration_ms\":0,\"position_ms\":0,\"source\":\"\"}}\n";
        assert_eq!(output, expected);
        let (tx, rx) = std::sync::mpsc::channel();
        let mut output = Vec::new();
        run_worker(
            Cursor::new(INIT),
            &mut output,
            |_| Ok(FakeSource(VecDeque::new())),
            SystemClock,
            (tx, rx),
        )
        .unwrap();
        assert_eq!(String::from_utf8(output).unwrap().lines().count(), 1);
    }
}
