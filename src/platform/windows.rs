use std::collections::HashMap;

use windows::{
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as SessionManager,
        GlobalSystemMediaTransportControlsSessionMediaProperties as MediaProperties,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
    },
    Storage::Streams::DataReader,
    core::Interface,
};

use crate::{
    core::{Candidate, Source, select_candidate},
    model::{Config, Status, Track},
};

struct ArtworkCache {
    identity: String,
    art_url: Option<String>,
}

pub struct WindowsSource {
    manager: SessionManager,
    config: Config,
    artwork: HashMap<String, ArtworkCache>,
    last_error: Option<String>,
}

impl WindowsSource {
    pub fn new(config: &Config) -> windows::core::Result<Self> {
        let manager = SessionManager::RequestAsync()?.join()?;
        Ok(Self {
            manager,
            config: config.clone(),
            artwork: HashMap::new(),
            last_error: None,
        })
    }

    fn read_snapshot(&mut self) -> windows::core::Result<Option<Track>> {
        let current = self.manager.GetCurrentSession().ok();
        let mut sessions = Vec::new();
        if let Some(current) = &current {
            sessions.push(current.clone());
        }
        let all = self.manager.GetSessions()?;
        for index in 0..all.Size()? {
            let session = all.GetAt(index)?;
            if current
                .as_ref()
                .is_some_and(|current| current.as_raw() == session.as_raw())
            {
                continue;
            }
            sessions.push(session);
        }
        let mut candidates = Vec::new();
        for (order, session) in sessions.into_iter().enumerate() {
            if let Ok(candidate) = read_session(session, order, &mut self.artwork) {
                candidates.push(candidate);
            }
        }
        Ok(select_candidate(candidates, &self.config))
    }
}

impl Source for WindowsSource {
    fn snapshot(&mut self) -> Option<Track> {
        match self.read_snapshot() {
            Ok(track) => {
                if self.last_error.take().is_some() {
                    eprintln!("conexión GSMTC recuperada");
                }
                track
            }
            Err(error) => {
                let message = error.to_string();
                if self.last_error.as_deref() != Some(&message) {
                    eprintln!("error leyendo GSMTC: {message}");
                    self.last_error = Some(message);
                }
                None
            }
        }
    }
}

fn read_session(
    session: Session,
    stable_order: usize,
    artwork: &mut HashMap<String, ArtworkCache>,
) -> windows::core::Result<Candidate> {
    let aumid = session.SourceAppUserModelId()?.to_string_lossy();
    let properties = session.TryGetMediaPropertiesAsync()?.join()?;
    let timeline = session.GetTimelineProperties()?;
    let playback = session.GetPlaybackInfo()?;
    let artist = properties.Artist()?.to_string_lossy();
    let artist = if artist.is_empty() {
        properties.AlbumArtist()?.to_string_lossy()
    } else {
        artist
    };
    let status = match playback.PlaybackStatus()? {
        PlaybackStatus::Playing => Status::Playing,
        PlaybackStatus::Paused => Status::Paused,
        _ => Status::Stopped,
    };
    let data = SessionData {
        source: source_id(&aumid),
        title: properties.Title()?.to_string_lossy(),
        artist,
        album: properties.AlbumTitle()?.to_string_lossy(),
        status,
        start: timeline.StartTime()?.Duration,
        end: timeline.EndTime()?.Duration,
        position: timeline.Position()?.Duration,
    };
    let identity = format!(
        "{}\0{}\0{}\0{}",
        data.title, data.artist, data.album, data.source
    );
    let art_url = match artwork.get(&aumid) {
        Some(cache) if cache.identity == identity => cache.art_url.clone(),
        _ => {
            let art_url = read_thumbnail(&properties).unwrap_or(None);
            artwork.insert(
                aumid.clone(),
                ArtworkCache {
                    identity,
                    art_url: art_url.clone(),
                },
            );
            art_url
        }
    };
    let mut candidate = candidate_from_data(data, aumid, stable_order);
    candidate.track.art_url = art_url;
    Ok(candidate)
}

fn read_thumbnail(properties: &MediaProperties) -> windows::core::Result<Option<String>> {
    let Ok(thumbnail) = properties.Thumbnail() else {
        return Ok(None);
    };
    let stream = thumbnail.OpenReadAsync()?.join()?;
    let size = stream.Size()?;
    if size == 0 || size > crate::artwork::MAX_ART_BYTES as u64 {
        return Ok(None);
    }
    let reader = DataReader::CreateDataReader(&stream)?;
    let loaded = reader.LoadAsync(size as u32)?.join()?;
    let mut bytes = vec![0; loaded as usize];
    reader.ReadBytes(&mut bytes)?;
    Ok(crate::artwork::data_uri(&bytes))
}

struct SessionData {
    source: String,
    title: String,
    artist: String,
    album: String,
    status: Status,
    start: i64,
    end: i64,
    position: i64,
}
fn candidate_from_data(data: SessionData, stable_id: String, stable_order: usize) -> Candidate {
    Candidate {
        track: Track {
            status: data.status,
            title: data.title,
            artist: data.artist,
            album: data.album,
            art_url: None,
            duration_ms: ticks_to_ms(data.end.saturating_sub(data.start)),
            position_ms: ticks_to_ms(data.position.saturating_sub(data.start)),
            source: data.source,
        },
        stable_id,
        stable_order,
    }
}
fn ticks_to_ms(ticks: i64) -> u64 {
    if ticks <= 0 { 0 } else { ticks as u64 / 10_000 }
}

fn source_id(aumid: &str) -> String {
    let lower = aumid.to_ascii_lowercase();
    for (needle, id) in [
        ("spotify", "spotify"),
        ("firefox", "firefox"),
        ("chrome", "chromium"),
        ("chromium", "chromium"),
        ("edge", "edge"),
        ("vlc", "vlc"),
        ("music", "music"),
        ("media player", "music"),
    ] {
        if lower.contains(needle) {
            return id.into();
        }
    }
    lower
        .rsplit(['.', '!', '/', '\\'])
        .map(str::trim)
        .map(|part| {
            part.chars()
                .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
                .collect::<String>()
        })
        .find(|part| !part.is_empty())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_known_aumids_and_fallback() {
        for (aumid, expected) in [
            ("Spotify.exe", "spotify"),
            ("Mozilla.Firefox", "firefox"),
            ("Chrome.App", "chromium"),
            ("Chromium.App", "chromium"),
            ("Microsoft.Edge", "edge"),
            ("VideoLAN.VLC", "vlc"),
            ("Microsoft.ZuneMusic", "music"),
            ("Company.Product.Player", "player"),
        ] {
            assert_eq!(source_id(aumid), expected);
        }
    }
    #[test]
    fn converts_ticks_and_negative_ranges() {
        assert_eq!(ticks_to_ms(42_000_000), 4200);
        assert_eq!(ticks_to_ms(-1), 0);
        let candidate = candidate_from_data(
            SessionData {
                source: "spotify".into(),
                title: "Song".into(),
                artist: "Artist".into(),
                album: "Album".into(),
                status: Status::Playing,
                start: 20_000,
                end: 10_000,
                position: 10_000,
            },
            "id".into(),
            0,
        );
        assert_eq!(candidate.track.duration_ms, 0);
        assert_eq!(candidate.track.position_ms, 0);
    }

    #[test]
    fn converts_complete_session_dto() {
        let candidate = candidate_from_data(
            SessionData {
                source: "firefox".into(),
                title: "Song".into(),
                artist: "Artist".into(),
                album: "Album".into(),
                status: Status::Paused,
                start: 10_000,
                end: 1_800_010_000,
                position: 420_010_000,
            },
            "id".into(),
            2,
        );
        assert_eq!(candidate.track.duration_ms, 180_000);
        assert_eq!(candidate.track.position_ms, 42_000);
        assert_eq!(candidate.track.source, "firefox");
        assert_eq!(candidate.stable_order, 2);
    }
}
