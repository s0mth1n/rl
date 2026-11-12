//! Test helper: copies stdin to the clipboard the way a password manager does, with the
//! x-kde-passwordManagerHint type offered next to the text. Stays alive 3 seconds to serve pastes.
use std::io::{Read, Write};
use std::time::{Duration, Instant};
use wayland_client::protocol::{wl_registry, wl_seat::WlSeat};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::ZwlrDataControlOfferV1,
    zwlr_data_control_source_v1::{self, ZwlrDataControlSourceV1},
};

struct S {
    seat: Option<WlSeat>,
    mgr: Option<ZwlrDataControlManagerV1>,
    text: String,
}
impl Dispatch<wl_registry::WlRegistry, ()> for S {
    fn event(
        s: &mut Self,
        r: &wl_registry::WlRegistry,
        e: wl_registry::Event,
        _: &(),
        _: &Connection,
        q: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = e
        {
            match interface.as_str() {
                "wl_seat" => s.seat = Some(r.bind(name, 1, q, ())),
                "zwlr_data_control_manager_v1" => s.mgr = Some(r.bind(name, 2.min(version), q, ())),
                _ => {}
            }
        }
    }
}
impl Dispatch<WlSeat, ()> for S {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: wayland_client::protocol::wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<ZwlrDataControlManagerV1, ()> for S {
    fn event(
        _: &mut Self,
        _: &ZwlrDataControlManagerV1,
        _: wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<ZwlrDataControlDeviceV1, ()> for S {
    fn event(
        _: &mut Self,
        _: &ZwlrDataControlDeviceV1,
        _: zwlr_data_control_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
    wayland_client::event_created_child!(S, ZwlrDataControlDeviceV1, [zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ())]);
}
impl Dispatch<ZwlrDataControlOfferV1, ()> for S {
    fn event(
        _: &mut Self,
        _: &ZwlrDataControlOfferV1,
        _: wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_offer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<ZwlrDataControlSourceV1, ()> for S {
    fn event(
        s: &mut Self,
        _: &ZwlrDataControlSourceV1,
        e: zwlr_data_control_source_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_data_control_source_v1::Event::Send { fd, .. } = e {
            let mut file = std::fs::File::from(fd);
            if let Ok(path) = std::env::var("ZL_TEST_COPY_STARTED") {
                let _ = std::fs::write(path, "started");
            }
            if let Ok(ms) = std::env::var("ZL_TEST_COPY_DELAY_MS") {
                std::thread::sleep(Duration::from_millis(ms.parse().unwrap_or(0)));
            }
            let _ = file.write_all(s.text.as_bytes());
        }
    }
}

fn main() {
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text).unwrap();
    let conn = Connection::connect_to_env().unwrap();
    let mut q = conn.new_event_queue::<S>();
    let qh = q.handle();
    conn.display().get_registry(&qh, ());
    let mut s = S {
        seat: None,
        mgr: None,
        text,
    };
    q.roundtrip(&mut s).unwrap();
    let dev = s
        .mgr
        .as_ref()
        .unwrap()
        .get_data_device(s.seat.as_ref().unwrap(), &qh, ());
    let src = s.mgr.as_ref().unwrap().create_data_source(&qh, ());
    src.offer("text/plain".into());
    if std::env::var_os("ZL_TEST_COPY_NO_HINT").is_none() {
        src.offer(
            std::env::var("ZL_TEST_COPY_HINT")
                .unwrap_or_else(|_| "x-kde-passwordManagerHint".into()),
        );
    }
    dev.set_selection(Some(&src));
    let end = Instant::now() + Duration::from_secs(3);
    while Instant::now() < end {
        q.roundtrip(&mut s).unwrap();
        std::thread::sleep(Duration::from_millis(100));
    }
}
