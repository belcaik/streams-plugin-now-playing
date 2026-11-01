pub mod core;
pub mod model;
pub mod protocol;

use std::{error::Error, io};

fn main() -> Result<(), Box<dyn Error>> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let _init = protocol::read_init(&mut reader)?;
    protocol::wait_for_eof(&mut reader)?;
    Ok(())
}
