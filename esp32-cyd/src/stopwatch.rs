use rustzx_core::host::Stopwatch;
use core::time::Duration;

pub struct InstantStopwatch {

}

impl Default for InstantStopwatch {
    fn default() -> Self {
        Self { }
    }
}

impl Stopwatch for InstantStopwatch {
    fn new() -> Self {
        Self::default()
    }

    fn measure(&self) -> Duration {
        Duration::from_millis(100)
    }
}