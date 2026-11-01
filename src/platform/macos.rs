use std::{error::Error, io, process::Command};

use serde::Deserialize;

use crate::{
    core::{Candidate, Source, select_candidate},
    model::{Config, Status, Track},
};

const SCRIPT: &str = r#"
const sessions = [];
for (const [source, name] of [["spotify", "Spotify"], ["music", "Music"]]) {
  try {
    if (!Application(name).running()) continue;
    const app = Application(name);
    const track = app.currentTrack;
    sessions.push({
      source, status: String(app.playerState()), name: track.name(),
      artist: track.artist(), album: track.album(),
      duration: source === "spotify" ? track.duration() : track.duration() * 1000,
      position: app.playerPosition() * 1000
    });
  } catch (_) {}
}
JSON.stringify(sessions);
"#;

pub struct MacOsSource {
    config: Config,
    last_error: Option<String>,
}

impl MacOsSource {
    pub fn new(config: &Config) -> Self {
        Self {
            config: config.clone(),
            last_error: None,
        }
    }

    fn read_snapshot(&self) -> Result<Option<Track>, Box<dyn Error>> {
        let output = Command::new("osascript")
            .args(["-l", "JavaScript", "-e", SCRIPT])
            .output()?;
        if !output.status.success() {
            return Err(
                io::Error::other(String::from_utf8_lossy(&output.stderr).into_owned()).into(),
            );
        }
        let candidates = parse_candidates(&String::from_utf8_lossy(&output.stdout))?;
        Ok(select_candidate(candidates, &self.config))
    }
}

impl Source for MacOsSource {
    fn snapshot(&mut self) -> Option<Track> {
        match self.read_snapshot() {
            Ok(track) => {
                if self.last_error.take().is_some() {
                    eprintln!("automatización recuperada");
                }
                track
            }
            Err(error) => {
                let message = error.to_string();
                if self.last_error.as_deref() != Some(&message) {
                    eprintln!("error leyendo automatización: {message}");
                    self.last_error = Some(message);
                }
                None
            }
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct AutomationTrack {
    source: String,
    status: String,
    name: String,
    artist: String,
    album: String,
    duration: f64,
    position: f64,
}

fn parse_candidates(json: &str) -> Result<Vec<Candidate>, serde_json::Error> {
    let tracks: Vec<AutomationTrack> = serde_json::from_str(json)?;
    Ok(tracks
        .into_iter()
        .enumerate()
        .map(|(stable_order, track)| {
            let status = match track.status.as_str() {
                "playing" => Status::Playing,
                "paused" => Status::Paused,
                _ => Status::Stopped,
            };
            Candidate {
                stable_id: track.source.clone(),
                stable_order,
                track: Track {
                    status,
                    title: track.name,
                    artist: track.artist,
                    album: track.album,
                    art_url: None,
                    duration_ms: milliseconds(track.duration),
                    position_ms: milliseconds(track.position),
                    source: track.source,
                },
            }
        })
        .collect())
}

fn milliseconds(value: f64) -> u64 {
    if value.is_finite() && value > 0.0 {
        value as u64
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spotify_and_music_with_milliseconds() {
        let json = r#"[
          {"source":"spotify","status":"playing","name":"Canción \"A\"","artist":"Uno","album":"Álbum","duration":180000,"position":42000},
          {"source":"music","status":"paused","name":"Otra","artist":"Dos","album":"Disco","duration":200000,"position":50000}
        ]"#;
        let candidates = parse_candidates(json).unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].track.title, "Canción \"A\"");
        assert_eq!(candidates[0].track.duration_ms, 180_000);
        assert_eq!(candidates[1].track.position_ms, 50_000);
        assert_eq!(candidates[1].track.status, Status::Paused);
    }

    #[test]
    fn unknown_states_and_empty_arrays_are_safe() {
        let json = r#"[{"source":"spotify","status":"unknown","duration":-1,"position":-2}]"#;
        let candidates = parse_candidates(json).unwrap();
        assert_eq!(candidates[0].track.status, Status::Stopped);
        assert_eq!(candidates[0].track.duration_ms, 0);
        assert!(parse_candidates("[]").unwrap().is_empty());
        assert!(parse_candidates("not json").is_err());
    }
}
