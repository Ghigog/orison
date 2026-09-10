//! `orison-cli`: the headless playable client (migration plan Phase 5).
//!
//! A library as well as a binary, and deliberately: Phase 5's exit criteria
//! require the evaluation harness to run *against the CLI*, not against a
//! second implementation of it that could drift. [`shell::Shell`] reads lines
//! from any `BufRead` and writes to any `Write`, so a scripted transcript
//! plays through exactly the code a person types into.

pub mod campaign;
pub mod config;
pub mod error;
pub mod shell;
