use std::io;

use crate::{core::Source, model::Config};

#[cfg(target_os = "linux")]
mod linux;

#[cfg(not(target_os = "linux"))]
struct StoppedSource;
#[cfg(not(target_os = "linux"))]
impl Source for StoppedSource {
    fn snapshot(&mut self) -> Option<crate::model::Track> {
        None
    }
}

pub fn create(config: &Config) -> io::Result<Box<dyn Source>> {
    #[cfg(target_os = "linux")]
    return linux::LinuxSource::new(config)
        .map(|source| Box::new(source) as Box<dyn Source>)
        .map_err(|error| io::Error::other(error.to_string()));

    #[cfg(not(target_os = "linux"))]
    Ok(Box::new(StoppedSource))
}
