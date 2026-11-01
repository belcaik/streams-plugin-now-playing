use std::io;

use crate::{
    core::Source,
    model::{Config, Track},
};

struct StoppedSource;
impl Source for StoppedSource {
    fn snapshot(&mut self) -> Option<Track> {
        None
    }
}

pub fn create(_config: &Config) -> io::Result<Box<dyn Source>> {
    Ok(Box::new(StoppedSource))
}
