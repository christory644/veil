#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

//! Safe FFI wrapper around libghosty for terminal emulation.

#[cfg(not(no_libghosty))]
mod ffi {
    // libghosty FFI bindings will go here.
}
