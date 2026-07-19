//! Safe platform filesystem primitives that isolate operating-system FFI.

mod replacement;

pub use replacement::replace_file;
