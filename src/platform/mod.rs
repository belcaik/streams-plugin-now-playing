use std::io;

use crate::{core::Source, model::Config};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
compile_error!("plataforma no soportada");

pub fn create(config: &Config) -> io::Result<Box<dyn Source>> {
    #[cfg(target_os = "linux")]
    return linux::LinuxSource::new(config)
        .map(|source| Box::new(source) as Box<dyn Source>)
        .map_err(|error| io::Error::other(error.to_string()));
    #[cfg(target_os = "windows")]
    return windows::WindowsSource::new(config)
        .map(|source| Box::new(source) as Box<dyn Source>)
        .map_err(|error| io::Error::other(error.to_string()));
    #[cfg(target_os = "macos")]
    return Ok(Box::new(macos::MacOsSource::new(config)));
}
