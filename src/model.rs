use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Playing,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub status: Status,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: Option<String>,
    pub duration_ms: u64,
    pub position_ms: u64,
    pub source: String,
}

impl Track {
    pub fn clamp_position(&mut self) {
        if self.duration_ms > 0 {
            self.position_ms = self.position_ms.min(self.duration_ms);
        }
    }

    pub fn stopped() -> Self {
        Self {
            status: Status::Stopped,
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            art_url: None,
            duration_ms: 0,
            position_ms: 0,
            source: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(default = "default_poll_ms")]
    pub poll_ms: u64,
    #[serde(default = "default_priority")]
    pub priority: Vec<String>,
    #[serde(default)]
    pub ignore: Vec<String>,
}

fn default_poll_ms() -> u64 {
    1000
}

fn default_priority() -> Vec<String> {
    ["spotify", "firefox", "chromium", "vlc", "music"]
        .map(String::from)
        .to_vec()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            poll_ms: default_poll_ms(),
            priority: default_priority(),
            ignore: Vec::new(),
        }
    }
}

impl Config {
    pub fn normalize(&mut self) {
        if self.poll_ms == 0 {
            self.poll_ms = 1000;
        }
        self.poll_ms = self.poll_ms.clamp(100, 60_000);
        normalize_list(&mut self.priority);
        normalize_list(&mut self.ignore);
    }
}

fn normalize_list(values: &mut [String]) {
    for value in values {
        *value = value.trim().to_owned();
        value.make_ascii_lowercase();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_match_plugin_manifest() {
        let config = Config::default();
        assert_eq!(config.poll_ms, 1000);
        assert_eq!(
            config.priority,
            ["spotify", "firefox", "chromium", "vlc", "music"]
        );
        assert!(config.ignore.is_empty());
    }

    #[test]
    fn config_normalizes_zero_and_poll_bounds() {
        let mut config = Config {
            poll_ms: 0,
            ..Config::default()
        };
        config.normalize();
        assert_eq!(config.poll_ms, 1000);
        config.poll_ms = 1;
        config.normalize();
        assert_eq!(config.poll_ms, 100);
        config.poll_ms = 100_000;
        config.normalize();
        assert_eq!(config.poll_ms, 60_000);
    }

    #[test]
    fn config_normalizes_lists_with_ascii_case_and_trim() {
        let mut config = Config {
            priority: vec![" Spotify ".into(), "FIREFOX".into()],
            ignore: vec![" VLC\t".into()],
            ..Config::default()
        };
        config.normalize();
        assert_eq!(config.priority, ["spotify", "firefox"]);
        assert_eq!(config.ignore, ["vlc"]);
    }

    #[test]
    fn stopped_track_has_canonical_empty_fields() {
        let track = Track::stopped();
        assert_eq!(track.status, Status::Stopped);
        assert_eq!(track.art_url, None);
        assert_eq!(track.duration_ms, 0);
        assert!(track.title.is_empty());
    }
}
