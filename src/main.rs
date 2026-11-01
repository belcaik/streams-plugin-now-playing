#[cfg(any(target_os = "linux", target_os = "windows"))]
mod artwork;
pub mod core;
pub mod model;
mod platform;
pub mod protocol;

use std::{error::Error, io, sync::mpsc};

use crate::core::{SystemClock, run_worker};

fn main() -> Result<(), Box<dyn Error>> {
    let (tx, rx) = mpsc::channel();
    run_worker(
        io::BufReader::new(io::stdin()),
        &mut io::stdout().lock(),
        platform::create,
        SystemClock,
        (tx, rx),
    )?;
    Ok(())
}
