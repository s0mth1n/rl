//! Test-only helpers: tests in one binary share process env, so every test
//! that touches XDG vars or env-backed state holds this lock for its body.
use std::sync::Mutex;

pub static ENV_LOCK: Mutex<()> = Mutex::new(());

/// DejaVu Sans for pixel tests. None where it is not installed.
pub fn font() -> Option<rusttype::Font<'static>> {
    let font = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    ]
    .iter()
    .find_map(|path| std::fs::read(path).ok())
    .and_then(rusttype::Font::try_from_vec);
    if font.is_none() {
        eprintln!("skipping font-dependent test: DejaVu Sans is not installed");
    }
    font
}
