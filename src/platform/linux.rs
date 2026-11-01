use std::{collections::HashMap, fs::File, io::Read};

use base64::Engine;
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
    let status = match player.playback_status().await?.as_str() {
        "Playing" => Status::Playing,
        "Paused" => Status::Paused,
        _ => Status::Stopped,
    };
    let position = player.position().await?;
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
    Ok(Candidate {
        track,
        stable_id: name,
        stable_order,
    })
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
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 1024 * 1024 {
        return None;
    }
    let mime = image_mime(&bytes)?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
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

pub(crate) fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}
