//! Test helper: a virtual pointer for headless compositors that have no input devices.
//! Reads commands from stdin: `m X Y` (absolute move), `p` (press left), `r` (release left), `rp`/`rr` (right button),
//! `a DY` (scroll wheel), `s SECONDS` (sleep), `w W H` (output size, default 1280 800). Uses wlr-virtual-pointer.
use std::io::BufRead;
use wayland_client::protocol::{wl_registry, wl_seat::WlSeat};
use wayland_client::{delegate_noop, Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};

#[derive(Default)]
struct S {
    seat: Option<WlSeat>,
    mgr: Option<ZwlrVirtualPointerManagerV1>,
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
                "zwlr_virtual_pointer_manager_v1" => s.mgr = Some(reg.bind(name, 1, qh, ())),
                _ => {}
            }
        }
    }
}
delegate_noop!(S: ignore WlSeat);
delegate_noop!(S: ignore ZwlrVirtualPointerManagerV1);
delegate_noop!(S: ignore ZwlrVirtualPointerV1);

fn main() {
    let conn = Connection::connect_to_env().expect("wayland");
    let mut q = conn.new_event_queue();
    let qh = q.handle();
    let mut s = S::default();
    conn.display().get_registry(&qh, ());
    q.roundtrip(&mut s).unwrap();
    let ptr = s
        .mgr
        .as_ref()
        .expect("no virtual pointer manager")
        .create_virtual_pointer(s.seat.as_ref(), &qh, ());
    let (mut ew, mut eh) = (1280u32, 800u32);
    let mut t = 0u32;
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let f: Vec<&str> = line.split_whitespace().collect();
        t += 10;
        match f.as_slice() {
            ["m", x, y] => {
                ptr.motion_absolute(t, x.parse().unwrap(), y.parse().unwrap(), ew, eh);
                ptr.frame();
            }
            ["p"] => {
                ptr.button(
                    t,
                    0x110,
                    wayland_client::protocol::wl_pointer::ButtonState::Pressed,
                );
                ptr.frame();
            }
            ["r"] => {
                ptr.button(
                    t,
                    0x110,
                    wayland_client::protocol::wl_pointer::ButtonState::Released,
                );
                ptr.frame();
            }
            ["rp"] => {
                ptr.button(
                    t,
                    0x111,
                    wayland_client::protocol::wl_pointer::ButtonState::Pressed,
                );
                ptr.frame();
            }
            ["rr"] => {
                ptr.button(
                    t,
                    0x111,
                    wayland_client::protocol::wl_pointer::ButtonState::Released,
                );
                ptr.frame();
            }
            ["q"] => {
                // right button press
                ptr.button(
                    t,
                    0x111,
                    wayland_client::protocol::wl_pointer::ButtonState::Pressed,
                );
                ptr.frame();
            }
            ["u"] => {
                // right button release
                ptr.button(
                    t,
                    0x111,
                    wayland_client::protocol::wl_pointer::ButtonState::Released,
                );
                ptr.frame();
            }
            ["d"] => {
                // double click: two quick left press/release pairs
                for _ in 0..2 {
                    ptr.button(
                        t,
                        0x110,
                        wayland_client::protocol::wl_pointer::ButtonState::Pressed,
                    );
                    ptr.frame();
                    t += 10;
                    ptr.button(
                        t,
                        0x110,
                        wayland_client::protocol::wl_pointer::ButtonState::Released,
                    );
                    ptr.frame();
                    t += 60;
                }
            }
            ["a", dy] => {
                // scroll wheel: positive scrolls down
                ptr.axis(
                    t,
                    wayland_client::protocol::wl_pointer::Axis::VerticalScroll,
                    dy.parse().unwrap(),
                );
                ptr.frame();
            }
            ["s", secs] => {
                std::thread::sleep(std::time::Duration::from_secs_f32(secs.parse().unwrap()))
            }
            ["w", w, h] => {
                ew = w.parse().unwrap();
                eh = h.parse().unwrap();
            }
            _ => {}
        }
        conn.flush().unwrap();
        let _ = q.roundtrip(&mut s);
    }
}
