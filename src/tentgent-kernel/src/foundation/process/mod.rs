//! Checked individual Unix process identifiers and signal boundaries.

mod unix;

pub(crate) use unix::{is_unix_process_running, send_unix_process_signal, unix_pid};
