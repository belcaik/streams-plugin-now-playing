use std::{collections::HashMap, fs::File, io::Read};

use zbus::{Connection, zvariant::OwnedVariant};

use crate::{
    core::{Candidate, Source, select_candidate},
    model::{Config, Status, Track},
};

#[zbus::proxy(
    interface = "org.freedesktop.DBus",
    default_service = "org.freedesktop.DBus",
    default_path = "/org/freedesktop/DBus"
)]
trait DBus {
    fn list_names(&self) -> zbus::Result<Vec<String>>;
}

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait MprisPlayer {
    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, OwnedVariant>>;
    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn position(&self) -> zbus::Result<i64>;
}

pub struct LinuxSource {
    connection: Connection,
    config: Config,
    last_error: Option<String>,
}

impl LinuxSource {
    pub fn new(config: &Config) -> zbus::Result<Self> {
        let connection = zbus::block_on(Connection::session())?;
        Ok(Self {
            connection,
            config: config.clone(),
            last_error: None,
        })
    }

    fn read_snapshot(&self) -> zbus::Result<Option<Track>> {
        zbus::block_on(async {
            let names = DBusProxy::new(&self.connection).await?.list_names().await?;
            let mut names: Vec<_> = names
                .into_iter()
                .filter(|name| name.starts_with("org.mpris.MediaPlayer2."))
                .collect();
            names.sort();
            let mut candidates = Vec::new();
            for (order, name) in names.into_iter().enumerate() {
                if let Ok(candidate) = read_candidate(&self.connection, name, order).await {
                    candidates.push(candidate);
                }
            }
            Ok(select_candidate(candidates, &self.config))
        })
    }
}

impl Source for LinuxSource {
    fn snapshot(&mut self) -> Option<Track> {
        match self.read_snapshot() {
            Ok(track) => {
                if self.last_error.take().is_some() {
                    eprintln!("conexión MPRIS recuperada");
                }
                track
            }
            Err(error) => {
                let message = error.to_string();
                if self.last_error.as_deref() != Some(&message) {
                    eprintln!("error leyendo MPRIS: {message}");
                    self.last_error = Some(message);
                }
                if let Ok(connection) = zbus::block_on(Connection::session()) {
                    self.connection = connection;
                }
                None
            }
        }
    }
}

async fn read_candidate(
    connection: &Connection,
    name: String,
    stable_order: usize,
) -> zbus::Result<Candidate> {
    let player = MprisPlayerProxy::builder(connection)
        .destination(name.as_str())
        .build()
        .await?;
    let metadata = player.metadata().await?;
    let status = player.playback_status().await?;
    let position = player.position().await?;
    Ok(candidate_from_values(
        name,
        status,
        position,
        metadata,
        stable_order,
    ))
}

fn candidate_from_values(
    name: String,
    playback: String,
    position: i64,
    metadata: HashMap<String, OwnedVariant>,
    stable_order: usize,
) -> Candidate {
    let status = match playback.as_str() {
        "Playing" => Status::Playing,
        "Paused" => Status::Paused,
        _ => Status::Stopped,
    };
    let track = Track {
        status,
        title: metadata_string(&metadata, "xesam:title").unwrap_or_default(),
        artist: metadata_artists(&metadata).unwrap_or_default(),
        album: metadata_string(&metadata, "xesam:album").unwrap_or_default(),
        art_url: metadata_string(&metadata, "mpris:artUrl").and_then(|url| sanitize_art_url(&url)),
        duration_ms: metadata
            .get("mpris:length")
            .and_then(value_i64)
            .map(micros_to_ms)
            .unwrap_or(0),
        position_ms: micros_to_ms(position),
        source: source_id(&name),
    };
    Candidate {
        track,
        stable_id: name,
        stable_order,
    }
}

fn metadata_string(metadata: &HashMap<String, OwnedVariant>, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(|value| String::try_from(value.clone()).ok())
}
fn metadata_artists(metadata: &HashMap<String, OwnedVariant>) -> Option<String> {
    metadata
        .get("xesam:artist")
        .and_then(|value| Vec::<String>::try_from(value.clone()).ok())
        .map(|artists| artists.join(", "))
}
fn value_i64(value: &OwnedVariant) -> Option<i64> {
    i64::try_from(value.clone()).ok()
}
fn micros_to_ms(micros: i64) -> u64 {
    if micros <= 0 { 0 } else { micros as u64 / 1000 }
}
fn source_id(name: &str) -> String {
    name.strip_prefix("org.mpris.MediaPlayer2.")
        .unwrap_or(name)
        .split('.')
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .flat_map(char::to_lowercase)
        .collect()
}

fn sanitize_art_url(url: &str) -> Option<String> {
    if url.starts_with("https://") || url.starts_with("data:") {
        return Some(url.to_owned());
    }
    let path = decode_file_path(url)?;
    let file = File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take((crate::artwork::MAX_ART_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > crate::artwork::MAX_ART_BYTES {
        return None;
    }
    crate::artwork::data_uri(&bytes)
}

fn decode_file_path(url: &str) -> Option<std::path::PathBuf> {
    let encoded = url.strip_prefix("file://")?.as_bytes();
    let mut decoded = Vec::with_capacity(encoded.len());
    let mut index = 0;
    while index < encoded.len() {
        if encoded[index] == b'%' {
            let hex = encoded.get(index + 1..index + 3)?;
            let text = std::str::from_utf8(hex).ok()?;
            decoded.push(u8::from_str_radix(text, 16).ok()?);
            index += 3;
        } else {
            decoded.push(encoded[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded)
        .ok()
        .map(std::path::PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string(value: &str) -> OwnedVariant {
        OwnedVariant::from(zbus::Str::from(value))
    }
    fn strings(values: Vec<&str>) -> OwnedVariant {
        OwnedVariant::try_from(zbus::Array::from(values)).unwrap()
    }

    fn metadata() -> HashMap<String, OwnedVariant> {
        HashMap::from([
            ("xesam:title".into(), string("Canción 🎵")),
            ("xesam:artist".into(), strings(vec!["Uno", "Dos"])),
            ("xesam:album".into(), string("Álbum")),
            ("mpris:length".into(), OwnedVariant::from(180_000_000_i64)),
            ("mpris:artUrl".into(), string("https://example/art.jpg")),
        ])
    }

    #[test]
    fn parses_complete_metadata_and_multiple_artists() {
        let candidate = candidate_from_values(
            "org.mpris.MediaPlayer2.spotify".into(),
            "Playing".into(),
            42_000_000,
            metadata(),
            0,
        );
        assert_eq!(candidate.track.title, "Canción 🎵");
        assert_eq!(candidate.track.artist, "Uno, Dos");
        assert_eq!(candidate.track.album, "Álbum");
        assert_eq!(candidate.track.duration_ms, 180_000);
        assert_eq!(candidate.track.position_ms, 42_000);
    }

    #[test]
    fn tolerates_missing_wrong_and_negative_values() {
        let metadata = HashMap::from([
            ("xesam:title".into(), OwnedVariant::from(42_i64)),
            ("xesam:artist".into(), string("not an array")),
            ("mpris:length".into(), OwnedVariant::from(-1_i64)),
        ]);
        let candidate = candidate_from_values(
            "org.mpris.MediaPlayer2.vlc".into(),
            "unknown".into(),
            -2,
            metadata,
            0,
        );
        assert_eq!(candidate.track.title, "");
        assert_eq!(candidate.track.artist, "");
        assert_eq!(candidate.track.duration_ms, 0);
        assert_eq!(candidate.track.position_ms, 0);
        assert_eq!(candidate.track.status, Status::Stopped);
    }

    #[test]
    fn derives_stable_short_source_ids() {
        assert_eq!(source_id("org.mpris.MediaPlayer2.spotify"), "spotify");
        assert_eq!(
            source_id("org.mpris.MediaPlayer2.Firefox.instance_1"),
            "firefox"
        );
        assert_eq!(source_id("org.mpris.MediaPlayer2.vlc"), "vlc");
    }

    #[test]
    fn filters_remote_art_and_decodes_local_paths() {
        assert_eq!(
            sanitize_art_url("https://example/a"),
            Some("https://example/a".into())
        );
        assert_eq!(
            sanitize_art_url("data:image/png;base64,AA=="),
            Some("data:image/png;base64,AA==".into())
        );
        assert_eq!(sanitize_art_url("http://example/a"), None);
        assert_eq!(
            decode_file_path("file:///tmp/a%20b.png"),
            Some(std::path::PathBuf::from("/tmp/a b.png"))
        );
    }

    #[test]
    fn rejects_oversized_file() {
        let path = std::env::temp_dir().join(format!("now-playing-{}-art.png", std::process::id()));
        let mut bytes = vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10];
        bytes.resize(1024 * 1024 + 1, 0);
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(
            sanitize_art_url(&format!("file://{}", path.display())),
            None
        );
        std::fs::remove_file(path).unwrap();
    }
}
