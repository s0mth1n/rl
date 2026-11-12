//! Test helper: a virtual keyboard for headless compositors.
//! Commands on stdin: `t TEXT` types text (US layout), `k enter|bs|esc|ctrl-l|ctrl-k|ctrl-o|down|up|left|right`, `s SECONDS` sleeps.
use std::io::{BufRead, Write};
use std::os::fd::AsFd;
use wayland_client::protocol::{wl_registry, wl_seat::WlSeat};
use wayland_client::{delegate_noop, Connection, Dispatch, QueueHandle};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};

#[derive(Default)]
struct S {
    seat: Option<WlSeat>,
    mgr: Option<ZwpVirtualKeyboardManagerV1>,
}
impl Dispatch<wl_registry::WlRegistry, ()> for S {
    fn event(
        s: &mut Self,
        reg: &wl_registry::WlRegistry,
        ev: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = ev
        {
            match interface.as_str() {
                "wl_seat" => s.seat = Some(reg.bind(name, 1, qh, ())),
                "zwp_virtual_keyboard_manager_v1" => s.mgr = Some(reg.bind(name, 1, qh, ())),
                _ => {}
            }
        }
    }
}
delegate_noop!(S: ignore WlSeat);
delegate_noop!(S: ignore ZwpVirtualKeyboardManagerV1);
delegate_noop!(S: ignore ZwpVirtualKeyboardV1);

fn code(c: char) -> Option<(u32, bool)> {
    let row = |s: &str, base: u32, ch: char| s.find(ch).map(|i| base + i as u32);
    let lc = c.to_ascii_lowercase();
    if c.is_ascii_alphabetic() {
        let k = row("qwertyuiop", 16, lc)
            .or_else(|| row("asdfghjkl", 30, lc))
            .or_else(|| row("zxcvbnm", 44, lc))?;
        return Some((k, c.is_ascii_uppercase()));
    }
    if let Some(k) = row("1234567890", 2, c) {
        return Some((k, false));
    }
    Some(match c {
        ' ' => (57, false),
        '.' => (52, false),
        ',' => (51, false),
        '-' => (12, false),
        '?' => (53, true),
        '/' => (53, false),
        ':' => (39, true),
        ';' => (39, false),
        '_' => (12, true),
        '*' => (9, true),
        '#' => (4, true),
        '%' => (6, true),
        '^' => (7, true),
        '=' => (13, false),
        '+' => (13, true),
        '(' => (10, true),
        ')' => (11, true),
        '{' => (26, true),
        '}' => (27, true),
        '"' => (40, true),
        _ => return None,
    })
}

fn main() {
    let conn = Connection::connect_to_env().expect("wayland");
    let mut q = conn.new_event_queue();
    let qh = q.handle();
    let mut s = S::default();
    conn.display().get_registry(&qh, ());
    q.roundtrip(&mut s).unwrap();
    let kb = s
        .mgr
        .as_ref()
        .expect("no virtual keyboard manager")
        .create_virtual_keyboard(s.seat.as_ref().unwrap(), &qh, ());
    let ctx = xkbcommon::xkb::Context::new(0);
    let km = xkbcommon::xkb::Keymap::new_from_names(
        &ctx,
        "",
        "",
        "us",
        &std::env::var("ZL_TEST_XKB_VARIANT").unwrap_or_default(),
        Some(std::env::var("ZL_TEST_XKB_OPTIONS").unwrap_or_default()),
        0,
    )
    .expect("keymap");
    let text = km.get_as_string(xkbcommon::xkb::KEYMAP_FORMAT_TEXT_V1);
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(format!("/tmp/vkbd-{}.xkb", std::process::id()))
        .unwrap();
    f.write_all(text.as_bytes()).unwrap();
    f.write_all(&[0]).unwrap();
    kb.keymap(1, f.as_fd(), text.len() as u32 + 1);
    conn.flush().unwrap();
    let _ = q.roundtrip(&mut s);
    std::thread::sleep(std::time::Duration::from_millis(400));
    let mut t = 0u32;
    let mut key = |kb: &ZwpVirtualKeyboardV1, k: u32, shift: bool, ctrl: bool| {
        let mods = (shift as u32) | ((ctrl as u32) << 2);
        kb.modifiers(mods, 0, 0, 0);
        t += 10;
        kb.key(t, k, 1);
        t += 10;
        kb.key(t, k, 0);
        kb.modifiers(0, 0, 0, 0);
    };
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        if let Some(txt) = line.strip_prefix("t ") {
            for c in txt.chars() {
                if let Some((k, sh)) = code(c) {
                    key(&kb, k, sh, false);
                    conn.flush().unwrap();
                    let _ = q.roundtrip(&mut s);
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            }
        } else if let Some(k) = line.strip_prefix("k ") {
            match k.trim() {
                "compose" => key(&kb, 127, false, false),
                "apostrophe" => key(&kb, 40, false, false),
                "enter" => key(&kb, 28, false, false),
                "shift-enter" => key(&kb, 28, true, false),
                "tab" => key(&kb, 15, false, false),
                "shift-tab" => key(&kb, 15, true, false),
                "bs" => key(&kb, 14, false, false),
                "ctrl-bs" => key(&kb, 14, false, true),
                "esc" => key(&kb, 1, false, false),
                "ctrl-l" => key(&kb, 38, false, true),
                "ctrl-k" => key(&kb, 37, false, true),
                "ctrl-o" => key(&kb, 24, false, true),
                "ctrl-t" => key(&kb, 20, false, true),
                "ctrl-1" => key(&kb, 2, false, true),
                "ctrl-2" => key(&kb, 3, false, true),
                "ctrl-3" => key(&kb, 4, false, true),
                "ctrl-4" => key(&kb, 5, false, true),
                "ctrl-5" => key(&kb, 6, false, true),
                "ctrl-6" => key(&kb, 7, false, true),
                "ctrl-7" => key(&kb, 8, false, true),
                "ctrl-8" => key(&kb, 9, false, true),
                "ctrl-9" => key(&kb, 10, false, true),
                "ctrl-0" => key(&kb, 11, false, true),
                "ctrl-comma" => key(&kb, 51, false, true),
                "ctrl-enter" => key(&kb, 28, false, true),
                "ctrl-shift-t" => key(&kb, 20, true, true),
                "ctrl-shift-f" => key(&kb, 33, true, true),
                "ctrl-shift-c" => key(&kb, 46, true, true),
                "down" => key(&kb, 108, false, false),
                "up" => key(&kb, 103, false, false),
                "left" => key(&kb, 105, false, false),
                "right" => key(&kb, 106, false, false),
                "ctrl-v" => key(&kb, 47, false, true),
                "ctrl-c" => key(&kb, 46, false, true),
                "ctrl-x" => key(&kb, 45, false, true),
                "ctrl-u" => key(&kb, 22, false, true),
                "ctrl-up" => key(&kb, 103, false, true),
                "ctrl-down" => key(&kb, 108, false, true),
                "ctrl-b" => key(&kb, 48, false, true),
                "ctrl-p" => key(&kb, 25, false, true),
                "ctrl-s" => key(&kb, 31, false, true),
                "ctrl-n" => key(&kb, 49, false, true),
                "ctrl-j" => key(&kb, 36, false, true),
                "ctrl-left" => key(&kb, 105, false, true),
                "ctrl-right" => key(&kb, 106, false, true),
                "del" => key(&kb, 111, false, false),
                "home" => key(&kb, 102, false, false),
                "end" => key(&kb, 107, false, false),
                "pgup" => key(&kb, 104, false, false),
                "pgdn" => key(&kb, 109, false, false),
                _ => {}
            }
        } else if let Some(sec) = line.strip_prefix("s ") {
            std::thread::sleep(std::time::Duration::from_secs_f32(
                sec.trim().parse().unwrap(),
            ));
        }
        conn.flush().unwrap();
        let _ = q.roundtrip(&mut s);
    }
}
