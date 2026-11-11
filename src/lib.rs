#![no_std]

pub mod advice;
#[cfg(target_arch = "riscv32")]
pub mod display;
pub mod ui;
pub mod weather;
