#![no_std]

extern crate alloc;

pub mod advice;
#[cfg(target_arch = "riscv32")]
pub mod display;
#[cfg(target_arch = "riscv32")]
pub mod scene;
pub mod ui;
pub mod weather;
