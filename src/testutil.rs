//! Test-only helpers: tests in one binary share process env, so every test
//! that touches XDG vars or env-backed state holds this lock for its body.
use std::sync::Mutex;

pub static ENV_LOCK: Mutex<()> = Mutex::new(());
