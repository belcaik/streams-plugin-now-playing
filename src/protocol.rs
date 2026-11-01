use std::{
    error::Error,
    io::{self, BufRead, Write},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};

use crate::model::{Config, Track};

#[derive(Debug, Deserialize)]
pub struct Init {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub config: Config,
    pub data_dir: PathBuf,
}

#[derive(Serialize)]
pub struct EventLine {
    #[serde(rename = "type")]
    event_type: &'static str,
    data: Track,
}

impl EventLine {
    pub fn new(data: Track) -> Self {
        Self {
            event_type: "now_playing",
            data,
        }
    }
}

pub fn read_init(reader: &mut impl BufRead) -> Result<Init, Box<dyn Error>> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "falta la línea init").into());
    }
    let mut init: Init = serde_json::from_str(&line)?;
    if init.event_type != "init" {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "la primera línea no es init").into(),
        );
    }
    init.config.normalize();
    Ok(init)
}

pub fn wait_for_eof(reader: &mut impl BufRead) -> io::Result<()> {
    let mut line = String::new();
    while reader.read_line(&mut line)? != 0 {
        line.clear();
    }
    Ok(())
}

pub fn write_event(writer: &mut impl Write, track: &Track) -> Result<(), Box<dyn Error>> {
    serde_json::to_writer(&mut *writer, &EventLine::new(track.clone()))?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::model::Status;

    #[test]
    fn parses_complete_init() {
        let input = br#"{"type":"init","config":{"poll_ms":2500},"data_dir":"/tmp/data"}"#;
        let init = read_init(&mut Cursor::new(input)).unwrap();
        assert_eq!(init.config.poll_ms, 2500);
        assert_eq!(init.data_dir, PathBuf::from("/tmp/data"));
    }

    #[test]
    fn empty_config_uses_defaults() {
        let input = br#"{"type":"init","config":{},"data_dir":"."}"#;
        let init = read_init(&mut Cursor::new(input)).unwrap();
        assert_eq!(init.config, Config::default());
    }

    #[test]
    fn rejects_non_init_and_broken_json() {
        let wrong = br#"{"type":"event","config":{},"data_dir":"."}"#;
        assert!(read_init(&mut Cursor::new(wrong)).is_err());
        assert!(read_init(&mut Cursor::new(b"{")).is_err());
    }

    #[test]
    fn eof_immediately_after_init_exits_without_output() {
        let mut input = Cursor::new(br#"{"type":"init","config":{},"data_dir":"."}"#.to_vec());
        read_init(&mut input).unwrap();
        wait_for_eof(&mut input).unwrap();
        assert!(Vec::<u8>::new().is_empty());
    }

    fn example_track() -> Track {
        Track {
            status: Status::Playing,
            title: "Song".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            art_url: Some("https://example/art.jpg".into()),
            duration_ms: 180_000,
            position_ms: 42_000,
            source: "spotify".into(),
        }
    }

    #[test]
    fn serializes_playing_event_byte_for_byte() {
        let expected = "{\"type\":\"now_playing\",\"data\":{\"status\":\"playing\",\"title\":\"Song\",\"artist\":\"Artist\",\"album\":\"Album\",\"art_url\":\"https://example/art.jpg\",\"duration_ms\":180000,\"position_ms\":42000,\"source\":\"spotify\"}}\n";
        let mut output = Vec::new();
        write_event(&mut output, &example_track()).unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), expected);
    }

    #[test]
    fn serializes_stopped_event_byte_for_byte() {
        let expected = "{\"type\":\"now_playing\",\"data\":{\"status\":\"stopped\",\"title\":\"\",\"artist\":\"\",\"album\":\"\",\"art_url\":null,\"duration_ms\":0,\"position_ms\":0,\"source\":\"\"}}\n";
        let mut output = Vec::new();
        write_event(&mut output, &Track::stopped()).unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), expected);
    }
}
