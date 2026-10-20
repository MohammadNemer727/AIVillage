//! Single-source per-agent compute kernels for AI Village.
//!
//! Everything here is a pure function of explicit inputs: no world access, no allocation, no
//! floating point, and no panicking conversions. `sim-core` calls these kernels directly on the
//! CPU, and the same source is intended to compile unchanged for a GPU backend, so a feature
//! written here is written once for both.
//!
//! Kernel rules, enforced by lints where possible:
//!
//! - `#![no_std]` with only fixed-size arrays and `Copy` state.
//! - Integer arithmetic only, so results are bit-identical on every target.
//! - Only per-agent state is mutated; shared counters are reported back to the caller.
//! - Narrowing conversions saturate explicitly instead of panicking.
//! - Fixed-size arrays are traversed with indexed loops. rust-gpu cannot lower slice or array
//!   iterators, sub-slice ranges, or `copy_from_slice`, so those stay out of kernel code.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::expect_used, clippy::panic, clippy::unwrap_used)
)]
#![allow(
    clippy::manual_memcpy,
    clippy::needless_range_loop,
    reason = "Indexed loops are the form rust-gpu can compile; see the kernel rules."
)]

pub mod brain;
pub mod controller;
pub mod gpu;
mod hash;
mod math;
pub mod senses;

pub use hash::derive;
pub use math::{heading, integer_sqrt};
