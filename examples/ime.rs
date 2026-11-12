//! Test input method for a private headless compositor. Never run in a desktop session.
use std::io::{BufRead, Write};
use std::os::fd::{AsFd, FromRawFd};
use wayland_client::protocol::{
    wl_buffer::WlBuffer,
    wl_shm::{Format, WlShm},
    wl_shm_pool::WlShmPool,
};
use wayland_client::{
    delegate_noop,
    protocol::{wl_compositor::WlCompositor, wl_registry, wl_seat::WlSeat, wl_surface::WlSurface},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols_misc::zwp_input_method_v2::client::{
    zwp_input_method_manager_v2::ZwpInputMethodManagerV2,
    zwp_input_method_v2::{self, ZwpInputMethodV2},
    zwp_input_popup_surface_v2::{self, ZwpInputPopupSurfaceV2},
};

#[derive(Default)]
struct State {
    seat: Option<WlSeat>,
    manager: Option<ZwpInputMethodManagerV2>,
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    serial: u32,
    active: bool,
}
impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        s: &mut Self,
        r: &wl_registry::WlRegistry,
        e: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = e
        {
            match interface.as_str() {
                "wl_seat" => s.seat = Some(r.bind(name, 1, qh, ())),
                "wl_compositor" => s.compositor = Some(r.bind(name, 1, qh, ())),
                "wl_shm" => s.shm = Some(r.bind(name, 1, qh, ())),
                "zwp_input_method_manager_v2" => s.manager = Some(r.bind(name, 1, qh, ())),
                _ => {}
            }
        }
    }
}
impl Dispatch<ZwpInputMethodV2, ()> for State {
    fn event(
        s: &mut Self,
        _: &ZwpInputMethodV2,
        e: zwp_input_method_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwp_input_method_v2::Event;
        match e {
            Event::Activate => {
                s.active = true;
                println!("activate");
            }
            Event::Deactivate => {
                s.active = false;
                println!("deactivate");
            }
            Event::Done => {
                s.serial = s.serial.wrapping_add(1);
                println!("done {} active={}", s.serial, s.active);
            }
            Event::SurroundingText {
                text,
                cursor,
                anchor,
            } => println!("surrounding {text:?} cursor={cursor} anchor={anchor}"),
            Event::Unavailable => panic!("private seat already has an input method"),
            _ => {}
        }
    }
}
impl Dispatch<ZwpInputPopupSurfaceV2, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwpInputPopupSurfaceV2,
        e: zwp_input_popup_surface_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_input_popup_surface_v2::Event::TextInputRectangle {
            x,
            y,
            width,
            height,
        } = e
        {
            println!("rectangle {x} {y} {width} {height}");
        }
    }
}
delegate_noop!(State: ignore WlShm);
delegate_noop!(State: ignore WlShmPool);
delegate_noop!(State: ignore WlBuffer);
delegate_noop!(State: ignore WlSeat);
delegate_noop!(State: ignore WlSurface);
delegate_noop!(State: ignore WlCompositor);
delegate_noop!(State: ignore ZwpInputMethodManagerV2);
fn main() {
    assert!(
        std::env::var("ZL_TEST_PIDFILE").is_ok(),
        "requires private tests/headless.sh session"
    );
    let c = Connection::connect_to_env().unwrap();
    let mut q = c.new_event_queue();
    let qh = q.handle();
    let mut s = State::default();
    c.display().get_registry(&qh, ());
    q.roundtrip(&mut s).unwrap();
    let input = s
        .manager
        .as_ref()
        .expect("no input-method-v2 manager")
        .get_input_method(s.seat.as_ref().unwrap(), &qh, ());
    let surface = s.compositor.as_ref().unwrap().create_surface(&qh, ());
    let _popup = input.get_input_popup_surface(&surface, &qh, ());
    let fd = unsafe { libc::memfd_create(c"ime-test-popup".as_ptr(), 0) };
    assert!(fd >= 0);
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    file.write_all(&[0; 4 * 100 * 24]).unwrap();
    let pool = s
        .shm
        .as_ref()
        .unwrap()
        .create_pool(file.as_fd(), 4 * 100 * 24, &qh, ());
    let buffer = pool.create_buffer(0, 100, 24, 400, Format::Argb8888, &qh, ());
    surface.attach(Some(&buffer), 0, 0);
    surface.damage(0, 0, 100, 24);
    surface.commit();
    q.roundtrip(&mut s).unwrap();
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        q.roundtrip(&mut s).unwrap();
        let mut fields = line.splitn(4, ' ');
        match fields.next().unwrap_or("") {
            "preedit" => {
                let begin = fields.next().unwrap().parse().unwrap();
                let end = fields.next().unwrap().parse().unwrap();
                input.set_preedit_string(fields.next().unwrap_or("").into(), begin, end);
                input.commit(s.serial);
            }
            "commit" => {
                input.commit_string(line.strip_prefix("commit ").unwrap_or("").into());
                input.commit(s.serial);
            }
            "cancel" => {
                input.set_preedit_string("".into(), 0, 0);
                input.commit(s.serial);
            }
            "delete" => {
                let before = fields.next().unwrap().parse().unwrap();
                let after = fields.next().unwrap().parse().unwrap();
                input.delete_surrounding_text(before, after);
                input.commit(s.serial);
            }
            "s" => std::thread::sleep(std::time::Duration::from_secs_f64(
                fields.next().unwrap().parse().unwrap(),
            )),
            _ => {}
        }
        q.roundtrip(&mut s).unwrap();
    }
    input.destroy();
    c.flush().unwrap();
}
