//! zl: a small Wayland launcher. One daemon, a unix socket, a layer-shell panel drawn on the CPU.
mod clip;
mod config;
mod icons;
mod items;
mod settings;
#[cfg(test)]
mod testutil;
mod ui;

use config::Config;
use items::{Action, App as DesktopApp, Item, Search};
use memmap2::MmapMut;
use rusttype::Font;
use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd, FromRawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::time::{Duration, Instant};
use wayland_client::protocol::{
    wl_buffer::WlBuffer,
    wl_compositor::WlCompositor,
    wl_keyboard::WlKeyboard,
    wl_output::WlOutput,
    wl_pointer::WlPointer,
    wl_registry::WlRegistry,
    wl_seat::WlSeat,
    wl_shm::{Format, WlShm},
    wl_shm_pool::WlShmPool,
    wl_surface::WlSurface,
};
use wayland_client::Proxy;
use wayland_client::{delegate_noop, Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
    zwlr_data_control_source_v1::{self, ZwlrDataControlSourceV1},
};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{Layer, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1},
};
use xkbcommon::xkb;

struct Out {
    obj: WlOutput,
    name: String,
    scale: u32,
}

struct App {
    cfg: Config,
    theme: config::Theme,
    icons: icons::Cache,
    font: Font<'static>,
    apps: Vec<DesktopApp>,
    items: Vec<Item>,
    query: String,
    sel: usize,
    top: usize,
    shown: bool,
    configured: bool,
    cur_h: u32,
    scale: u32,
    outputs: Vec<Out>,
    repeat_rate: i32,
    repeat_delay: i32,
    repeating: Option<(u32, Instant)>,
    pointer_over_row: bool,
    last_click: Option<(usize, Instant)>,
    settings_error: Option<String>,
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    shell: Option<ZwlrLayerShellV1>,
    seat: Option<WlSeat>,
    kb: Option<WlKeyboard>,
    ptr: Option<WlPointer>,
    surface: Option<WlSurface>,
    layer: Option<ZwlrLayerSurfaceV1>,
    buffer: Option<WlBuffer>,
    ctx: xkb::Context,
    xkb: Option<xkb::State>,
    clip: clip::History,
    dc_manager: Option<ZwlrDataControlManagerV1>,
    dc_device: Option<ZwlrDataControlDeviceV1>,
    offers: std::collections::HashMap<wayland_client::backend::ObjectId, Vec<String>>,
    reads: Vec<PipeRead>,
    own_source: Option<(ZwlrDataControlSourceV1, Vec<u8>)>,
}

/// A clipboard offer being read from its pipe.
struct PipeRead {
    file: std::fs::File,
    buf: Vec<u8>,
    started: Instant,
    image: bool,
    html: bool,
}

impl App {
    fn rows(&self) -> usize {
        self.items.len().min(self.cfg.max_rows).max(1)
    }

    fn refresh(&mut self) {
        let s = Search {
            cfg: &self.cfg,
            apps: &self.apps,
            history: &self.clip,
        };
        self.items = s.run(&self.query);
        self.sel = 0;
        self.top = 0;
        if let Some(message) = self.settings_error.take() {
            self.items.insert(
                0,
                Item {
                    title: message,
                    sub: "Check file permissions and try again".into(),
                    tag: "Error",
                    icon: None,
                    action: Action::View(self.query.clone()),
                },
            );
        }
    }

    fn ensure_visible(&mut self) {
        let rows = self.cfg.max_rows.min(self.items.len()).max(1);
        if self.sel < self.top {
            self.top = self.sel;
        }
        if self.sel >= self.top + rows {
            self.top = self.sel + 1 - rows;
        }
        self.top = self.top.min(self.items.len().saturating_sub(rows));
    }

    fn show(&mut self, qh: &QueueHandle<App>) {
        if self.shown {
            return;
        }
        if self.compositor.is_none() || self.shell.is_none() {
            return;
        }
        self.apps = items::scan_apps(&self.cfg);
        self.last_click = None;
        self.query.clear();
        self.refresh();
        let (Some(c), Some(sh)) = (&self.compositor, &self.shell) else {
            return;
        };
        let out = if self.cfg.monitor.is_empty() {
            None
        } else {
            self.outputs
                .iter()
                .find(|o| o.name == self.cfg.monitor)
                .map(|o| &o.obj)
        };
        self.scale = self
            .outputs
            .iter()
            .filter(|o| out.is_none_or(|x| *x == o.obj))
            .map(|o| o.scale)
            .max()
            .unwrap_or(1)
            .max(1);
        let s = c.create_surface(qh, ());
        let l = sh.get_layer_surface(&s, out, Layer::Overlay, "zl".into(), qh, ());
        self.cur_h = ui::panel_height(self.rows());
        l.set_size(self.cfg.width, self.cur_h);
        l.set_anchor(Anchor::empty());
        l.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        s.commit();
        self.surface = Some(s);
        self.layer = Some(l);
        self.shown = true;
        self.configured = false;
    }

    fn hide(&mut self) {
        self.repeating = None;
        if let Some(l) = self.layer.take() {
            l.destroy();
        }
        if let Some(b) = self.buffer.take() {
            b.destroy();
        }
        if let Some(s) = self.surface.take() {
            s.destroy();
        }
        self.shown = false;
        self.configured = false;
    }

    fn redraw(&mut self, qh: &QueueHandle<App>) {
        if !self.shown || !self.configured {
            return;
        }
        self.ensure_visible();
        let h = ui::panel_height(self.rows());
        if h != self.cur_h {
            self.cur_h = h;
            if let Some(l) = &self.layer {
                l.set_size(self.cfg.width, h);
            }
        }
        if self.shm.is_none() || self.surface.is_none() {
            return;
        }
        let s = self.scale;
        let rows = self.rows();
        for it in self.items.iter().skip(self.top).take(rows) {
            if let Some(n) = &it.icon {
                self.icons.ensure(n, ui::icon_px(s));
            }
        }
        let Some(surf) = &self.surface else { return };
        let mut cv = ui::Canvas::new(self.cfg.width * s, h * s);
        let v = ui::View {
            icons: &self.icons,
            query: &self.query,
            items: &self.items,
            sel: self.sel,
            top: self.top,
            rows: self.rows(),
            theme: self.theme,
            w: self.cfg.width,
            h,
            scale: s,
            font_size: self.cfg.font_size,
        };
        ui::draw(&mut cv, &self.font, &v);
        let size = cv.buf.len();
        let fd = unsafe { libc::memfd_create(c"zl".as_ptr(), 0) };
        if fd < 0 {
            return;
        }
        let file = unsafe { std::fs::File::from_raw_fd(fd) };
        if file.set_len(size as u64).is_err() {
            return;
        }
        let Ok(mut map) = (unsafe { MmapMut::map_mut(&file) }) else {
            return;
        };
        map.copy_from_slice(&cv.buf);
        let Some(shm) = &self.shm else { return };
        let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
        let buf = pool.create_buffer(
            0,
            cv.w as i32,
            cv.h as i32,
            (cv.w * 4) as i32,
            Format::Argb8888,
            qh,
            (),
        );
        pool.destroy();
        surf.set_buffer_scale(s as i32);
        surf.attach(Some(&buf), 0, 0);
        surf.damage_buffer(0, 0, cv.w as i32, cv.h as i32);
        surf.commit();
        if let Some(o) = self.buffer.replace(buf) {
            o.destroy();
        }
    }

    fn activate(&mut self, qh: &QueueHandle<App>) {
        let Some(it) = self.items.get(self.sel).cloned() else {
            return;
        };
        match it.action {
            Action::App(i) => {
                self.hide();
                let a = &self.apps[i];
                let mut cmd = items::exec_line(&a.exec);
                if a.terminal {
                    cmd = format!("{} {}", self.cfg.terminal, cmd);
                }
                launch_application(&cmd);
            }
            Action::Clip(t) => {
                self.hide();
                self.set_clipboard(t.as_bytes(), false, qh);
            }
            Action::ClipImg(id) => {
                if let Some(image) = self.clip.images.iter().find(|i| i.id == id) {
                    let png = image.png.clone();
                    self.hide();
                    self.set_clipboard(&png, true, qh);
                }
            }
            Action::View(view) => {
                self.query = view;
                self.refresh();
                self.redraw(qh);
            }
            Action::Setting(key) => {
                if !settings::step(&mut self.cfg, &key) {
                    self.settings_error = Some("Could not save settings".into());
                }
                if key == "clipboard_persist" && !self.cfg.clipboard_persist {
                    if let Err(error) = std::fs::remove_dir_all(clip::dir()) {
                        if error.kind() != std::io::ErrorKind::NotFound {
                            self.settings_error = Some("Could not remove saved clipboard".into());
                        }
                    }
                }
                if !self.cfg.clipboard_history {
                    self.reads.clear();
                }
                self.theme = self.cfg.theme();
                self.clip.max = self.cfg.clipboard_max;
                self.clip.trim();
                if self.cfg.clipboard_persist {
                    self.clip.save();
                }
                let error = self.settings_error.take();
                self.hide();
                self.show(qh);
                self.settings_error = error;
                self.query = "settings".into();
                self.refresh();
                self.redraw(qh);
            }
            Action::Exclude(name) => {
                if !settings::exclude(&mut self.cfg, &name) {
                    self.settings_error = Some("Could not save exclusions".into());
                }
                let sel = self.sel;
                self.refresh();
                self.sel = sel.min(self.items.len().saturating_sub(1));
                self.redraw(qh);
            }
            Action::ClearClip => {
                self.clip.clear();
                if self.cfg.clipboard_persist {
                    self.clip.save();
                }
                self.refresh();
                self.redraw(qh);
            }
        }
    }

    fn mv(&mut self, d: isize) {
        let n = self.items.len() as isize;
        if n > 0 {
            self.sel = (self.sel as isize + d).clamp(0, n - 1) as usize;
        }
    }

    fn key(&mut self, key: u32, qh: &QueueHandle<App>) -> bool {
        use xkbcommon::xkb::keysyms as k;
        let Some(st) = &self.xkb else { return false };
        let code = xkb::Keycode::new(key + 8);
        let sym = st.key_get_one_sym(code).raw();
        let utf8 = st.key_get_utf8(code);
        let ctrl = st.mod_name_is_active(xkb::MOD_NAME_CTRL, xkb::STATE_MODS_EFFECTIVE);
        let mut repeatable = true;
        match sym {
            k::KEY_Escape => {
                if !self.query.is_empty() {
                    self.query.clear();
                    self.refresh();
                    self.redraw(qh);
                } else {
                    self.hide();
                }
                return false;
            }
            k::KEY_Return | k::KEY_KP_Enter => {
                self.activate(qh);
                return false;
            }
            k::KEY_BackSpace => {
                self.query.pop();
                self.refresh();
            }
            k::KEY_Down | k::KEY_Tab => self.mv(1),
            k::KEY_Up | k::KEY_ISO_Left_Tab => self.mv(-1),
            k::KEY_Page_Down => self.mv(self.cfg.max_rows as isize),
            k::KEY_Page_Up => self.mv(-(self.cfg.max_rows as isize)),
            k::KEY_Home => self.sel = 0,
            k::KEY_End => self.sel = self.items.len().saturating_sub(1),
            _ if ctrl => match utf8.as_str() {
                "\u{15}" => {
                    self.query.clear();
                    self.refresh();
                }
                "\u{17}" => {
                    let t = self.query.trim_end().rfind(' ').map_or(0, |i| i + 1);
                    self.query.truncate(t);
                    self.refresh();
                }
                "\u{e}" | "\u{a}" => self.mv(1),
                "\u{10}" | "\u{b}" => self.mv(-1),
                "\u{7}" => {
                    self.hide();
                    return false;
                }
                _ => repeatable = false,
            },
            _ => {
                if !utf8.is_empty() && !utf8.chars().any(|c| c.is_control()) {
                    self.query.push_str(&utf8);
                    self.refresh();
                } else {
                    repeatable = false;
                }
            }
        }
        self.redraw(qh);
        repeatable
    }

    fn row_at(&self, y: f64) -> Option<usize> {
        let y = y as f32 - ui::HEAD;
        if y < 0.0 {
            return None;
        }
        let r = (y / ui::ROW) as usize;
        let i = self.top + r;
        if r < self.rows() && i < self.items.len() {
            Some(i)
        } else {
            None
        }
    }
}

impl App {
    fn init_clip(&mut self, qh: &QueueHandle<App>) {
        if let (Some(m), Some(s)) = (&self.dc_manager, &self.seat) {
            self.dc_device = Some(m.get_data_device(s, qh, ()));
        }
    }

    /// Offer text or PNG bytes as the current clipboard selection.
    fn set_clipboard(&mut self, bytes: &[u8], image: bool, qh: &QueueHandle<App>) {
        let (Some(m), Some(d)) = (&self.dc_manager, &self.dc_device) else {
            copy_bytes(bytes, image);
            return;
        };
        let src = m.create_data_source(qh, ());
        if image {
            src.offer("image/png".into());
        } else {
            for mime in [
                "text/plain;charset=utf-8",
                "text/plain",
                "UTF8_STRING",
                "STRING",
            ] {
                src.offer(mime.into());
            }
        }
        d.set_selection(Some(&src));
        if let Some((old, _)) = self.own_source.replace((src, bytes.to_vec())) {
            old.destroy();
        }
    }

    fn finish_reads(&mut self, qh: &QueueHandle<App>) {
        let mut i = 0;
        while i < self.reads.len() {
            let mut done = false;
            let mut discard = self.reads[i].started.elapsed() > Duration::from_secs(2);
            let mut chunk = [0u8; 8192];
            loop {
                match self.reads[i].file.read(&mut chunk) {
                    Ok(0) => {
                        done = true;
                        break;
                    }
                    Ok(n) => {
                        self.reads[i].buf.extend_from_slice(&chunk[..n]);
                        if self.reads[i].buf.len()
                            > if self.reads[i].image {
                                clip::MAX_IMAGE
                            } else {
                                clip::MAX_ENTRY
                            }
                        {
                            discard = true;
                            break;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => {
                        discard = true;
                        break;
                    }
                }
            }
            if discard {
                self.reads.swap_remove(i);
                continue;
            }
            if done {
                let r = self.reads.swap_remove(i);
                if r.image {
                    self.clip.push_image(&r.buf);
                } else if let Ok(t) = String::from_utf8(r.buf) {
                    self.clip
                        .push(&if r.html { clip::strip_html(&t) } else { t });
                }
                if self.cfg.clipboard_persist {
                    self.clip.save();
                }
                if self.shown && self.query.starts_with(';') {
                    self.refresh();
                    self.redraw(qh);
                }
            } else {
                i += 1;
            }
        }
    }
}

fn launch_application(cmd: &str) {
    let mut c = std::process::Command::new("sh");
    c.arg("-c")
        .arg(cmd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    unsafe {
        c.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let _ = c.spawn();
}

fn copy_bytes(bytes: &[u8], image: bool) {
    let mut cmd = std::process::Command::new("wl-copy");
    if image {
        cmd.args(["--type", "image/png"]);
    }
    if let Ok(mut child) = cmd.stdin(std::process::Stdio::piped()).spawn() {
        if let Some(mut input) = child.stdin.take() {
            let _ = input.write_all(bytes);
        }
        // Reap wl-copy when it exits so clipboard sets do not pile up as zombies.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

delegate_noop!(App: ignore WlCompositor);
delegate_noop!(App: ignore WlShm);
delegate_noop!(App: ignore WlShmPool);
delegate_noop!(App: ignore WlBuffer);
delegate_noop!(App: ignore ZwlrLayerShellV1);

delegate_noop!(App: ignore ZwlrDataControlManagerV1);

impl Dispatch<ZwlrDataControlDeviceV1, ()> for App {
    fn event(
        a: &mut Self,
        _: &ZwlrDataControlDeviceV1,
        ev: zwlr_data_control_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match ev {
            zwlr_data_control_device_v1::Event::DataOffer { id } => {
                // Offers abandoned without a selection (selection cleared) stay in this map;
                // keep it bounded.
                if a.offers.len() >= 64 {
                    a.offers.clear();
                }
                a.offers.insert(id.id(), Vec::new());
            }
            zwlr_data_control_device_v1::Event::Selection { id } => {
                let Some(offer) = id else { return };
                let mimes = a.offers.get(&offer.id()).cloned().unwrap_or_default();
                if !a.cfg.clipboard_history || a.reads.len() >= 8 {
                    a.offers.remove(&offer.id());
                    offer.destroy();
                    return;
                }
                let image = clip::pick_image(&mimes).is_some();
                if let Some(m) = clip::pick_mime(&mimes).or_else(|| clip::pick_image(&mimes)) {
                    let html = m == "text/html";
                    let mut fds = [0i32; 2];
                    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } == 0 {
                        let (r, w) = unsafe {
                            (
                                std::fs::File::from_raw_fd(fds[0]),
                                std::fs::File::from_raw_fd(fds[1]),
                            )
                        };
                        unsafe {
                            libc::fcntl(r.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK);
                        }
                        offer.receive(m, w.as_fd());
                        a.reads.push(PipeRead {
                            file: r,
                            buf: Vec::new(),
                            started: Instant::now(),
                            image,
                            html,
                        });
                    }
                }
                a.offers.remove(&offer.id());
                offer.destroy();
            }
            _ => {}
        }
    }
    wayland_client::event_created_child!(App, ZwlrDataControlDeviceV1, [
        zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ()),
    ]);
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for App {
    fn event(
        a: &mut Self,
        o: &ZwlrDataControlOfferV1,
        ev: zwlr_data_control_offer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_data_control_offer_v1::Event::Offer { mime_type } = ev {
            a.offers.entry(o.id()).or_default().push(mime_type);
        }
    }
}

impl Dispatch<ZwlrDataControlSourceV1, ()> for App {
    fn event(
        a: &mut Self,
        s: &ZwlrDataControlSourceV1,
        ev: zwlr_data_control_source_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match ev {
            zwlr_data_control_source_v1::Event::Send { fd, .. } => {
                if let Some((src, text)) = &a.own_source {
                    if src == s {
                        let mut f = std::fs::File::from(fd);
                        let data = text.clone();
                        std::thread::spawn(move || {
                            let _ = f.write_all(&data);
                        });
                    }
                }
            }
            zwlr_data_control_source_v1::Event::Cancelled => {
                if a.own_source.as_ref().is_some_and(|(src, _)| src == s) {
                    a.own_source = None;
                }
                s.destroy();
            }
            _ => {}
        }
    }
}

impl Dispatch<WlSurface, ()> for App {
    fn event(
        a: &mut Self,
        _: &WlSurface,
        ev: wayland_client::protocol::wl_surface::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wayland_client::protocol::wl_surface::Event::Enter { output } = ev {
            if let Some(o) = a.outputs.iter().find(|o| o.obj == output) {
                if o.scale != a.scale {
                    a.scale = o.scale.max(1);
                    a.redraw(qh);
                }
            }
        }
    }
}

impl Dispatch<WlOutput, ()> for App {
    fn event(
        a: &mut Self,
        o: &WlOutput,
        ev: wayland_client::protocol::wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use wayland_client::protocol::wl_output::Event;
        let Some(e) = a.outputs.iter_mut().find(|e| e.obj == *o) else {
            return;
        };
        match ev {
            Event::Scale { factor } => e.scale = factor.max(1) as u32,
            Event::Name { name } => e.name = name,
            _ => {}
        }
    }
}

impl Dispatch<WlRegistry, ()> for App {
    fn event(
        a: &mut Self,
        r: &WlRegistry,
        ev: wayland_client::protocol::wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wayland_client::protocol::wl_registry::Event::Global {
            name,
            interface,
            version,
        } = ev
        {
            match interface.as_str() {
                "wl_compositor" => a.compositor = Some(r.bind(name, 4.min(version), qh, ())),
                "wl_shm" => a.shm = Some(r.bind(name, 1, qh, ())),
                "zwlr_layer_shell_v1" => a.shell = Some(r.bind(name, 1, qh, ())),
                "zwlr_data_control_manager_v1" => {
                    a.dc_manager = Some(r.bind(name, 2.min(version), qh, ()))
                }
                "wl_seat" => a.seat = Some(r.bind(name, 5.min(version), qh, ())),
                "wl_output" => {
                    let o: WlOutput = r.bind(name, 4.min(version), qh, ());
                    a.outputs.push(Out {
                        obj: o,
                        name: String::new(),
                        scale: 1,
                    });
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<WlSeat, ()> for App {
    fn event(
        a: &mut Self,
        s: &WlSeat,
        ev: wayland_client::protocol::wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use wayland_client::protocol::wl_seat::Capability;
        if let wayland_client::protocol::wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(c),
        } = ev
        {
            if c.contains(Capability::Keyboard) {
                if a.kb.is_none() {
                    a.kb = Some(s.get_keyboard(qh, ()));
                }
            } else if let Some(k) = a.kb.take() {
                k.release();
            }
            if c.contains(Capability::Pointer) {
                if a.ptr.is_none() {
                    a.ptr = Some(s.get_pointer(qh, ()));
                }
            } else if let Some(p) = a.ptr.take() {
                p.release();
            }
        }
    }
}

impl Dispatch<WlKeyboard, ()> for App {
    fn event(
        a: &mut Self,
        _: &WlKeyboard,
        ev: wayland_client::protocol::wl_keyboard::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use wayland_client::protocol::wl_keyboard::{Event, KeyState};
        match ev {
            Event::Keymap { fd, size, .. } => {
                if let Ok(Some(km)) = unsafe {
                    xkb::Keymap::new_from_fd(
                        &a.ctx,
                        fd,
                        size as usize,
                        xkb::KEYMAP_FORMAT_TEXT_V1,
                        xkb::KEYMAP_COMPILE_NO_FLAGS,
                    )
                } {
                    a.xkb = Some(xkb::State::new(&km));
                }
            }
            Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                if let Some(s) = a.xkb.as_mut() {
                    s.update_mask(mods_depressed, mods_latched, mods_locked, 0, 0, group);
                }
            }
            Event::RepeatInfo { rate, delay } => {
                a.repeat_rate = rate;
                a.repeat_delay = delay;
            }
            Event::Key {
                key,
                state: WEnum::Value(KeyState::Pressed),
                ..
            } => {
                if a.key(key, qh) && a.repeat_rate > 0 {
                    a.repeating = Some((
                        key,
                        Instant::now() + Duration::from_millis(a.repeat_delay.max(0) as u64),
                    ));
                }
            }
            Event::Key {
                key,
                state: WEnum::Value(KeyState::Released),
                ..
            } => {
                if a.repeating.is_some_and(|r| r.0 == key) {
                    a.repeating = None;
                }
            }
            Event::Leave { .. } => a.repeating = None,
            _ => {}
        }
    }
}

impl Dispatch<WlPointer, ()> for App {
    fn event(
        a: &mut Self,
        _: &WlPointer,
        ev: wayland_client::protocol::wl_pointer::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use wayland_client::protocol::wl_pointer::{Axis, ButtonState, Event};
        match ev {
            Event::Motion { surface_y, .. } | Event::Enter { surface_y, .. } => {
                let hit = a.row_at(surface_y);
                a.pointer_over_row = hit.is_some();
                if let Some(i) = hit {
                    if i != a.sel {
                        a.sel = i;
                        a.redraw(qh);
                    }
                }
            }
            Event::Button {
                button: 272,
                state: WEnum::Value(ButtonState::Pressed),
                ..
            } => {
                // The pointer position is the selected row after the last motion.
                if a.shown && a.pointer_over_row {
                    let now = Instant::now();
                    let double = a.last_click.is_some_and(|(i, t)| {
                        i == a.sel && now.duration_since(t) < Duration::from_millis(400)
                    });
                    if a.cfg.single_click || double {
                        a.last_click = None;
                        a.activate(qh);
                    } else {
                        a.last_click = Some((a.sel, now));
                    }
                }
            }
            Event::Axis {
                axis: WEnum::Value(Axis::VerticalScroll),
                value,
                ..
            } => {
                let max_top = a.items.len().saturating_sub(a.rows());
                let step = if value > 0.0 { 1isize } else { -1 };
                a.top = (a.top as isize + step).clamp(0, max_top as isize) as usize;
                a.sel = a
                    .sel
                    .clamp(a.top, a.top + a.rows() - 1)
                    .min(a.items.len().saturating_sub(1));
                a.redraw(qh);
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, ()> for App {
    fn event(
        a: &mut Self,
        ls: &ZwlrLayerSurfaceV1,
        ev: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match ev {
            zwlr_layer_surface_v1::Event::Configure { serial, .. } => {
                ls.ack_configure(serial);
                a.configured = true;
                a.redraw(qh);
            }
            zwlr_layer_surface_v1::Event::Closed => a.hide(),
            _ => {}
        }
    }
}

fn sock() -> String {
    format!(
        "{}/zl.sock",
        std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into())
    )
}

fn load_font(cfg: &Config) -> Font<'static> {
    let mut paths: Vec<String> = Vec::new();
    if !cfg.font.is_empty() {
        paths.push(cfg.font.clone());
    }
    if let Ok(o) = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", "sans"])
        .output()
    {
        paths.push(String::from_utf8_lossy(&o.stdout).into_owned());
    }
    paths.push("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into());
    for p in paths {
        if let Some(f) = std::fs::read(&p).ok().and_then(|d| Font::try_from_vec(d)) {
            return f;
        }
    }
    eprintln!("zl: no usable font found, set `font = \"/path/to/font.ttf\"` in the config");
    std::process::exit(1);
}

fn main() {
    if let Some(cmd) = std::env::args().nth(1) {
        match cmd.as_str() {
            "show" | "hide" | "toggle" | "quit" | "clear-clipboard" => {}
            "themes" => {
                println!("{}", config::THEMES.join("\n"));
                return;
            }
            _ => {
                eprintln!("usage: zl [show|hide|toggle|clear-clipboard|quit|themes]  (no argument starts the daemon)");
                std::process::exit(2);
            }
        }
        match UnixStream::connect(sock()) {
            Ok(mut s) => {
                let _ = s.write_all(cmd.as_bytes());
            }
            Err(_) => {
                eprintln!("zl: daemon is not running, start it with `zl`");
                std::process::exit(1);
            }
        }
        return;
    }
    let cfg = Config::load();
    if UnixStream::connect(sock()).is_ok() {
        eprintln!("zl: already running");
        std::process::exit(1);
    }
    let _ = std::fs::remove_file(sock());
    let listener = UnixListener::bind(sock()).expect("cannot create the control socket");
    listener.set_nonblocking(true).unwrap();
    let conn = Connection::connect_to_env().expect("no Wayland display");
    let mut queue = conn.new_event_queue::<App>();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut app = App {
        theme: cfg.theme(),
        icons: icons::Cache::new(&cfg.icon_theme, &cfg.emoji_font),
        font: load_font(&cfg),
        apps: items::scan_apps(&cfg),
        items: vec![],
        query: String::new(),
        sel: 0,
        top: 0,
        shown: false,
        configured: false,
        cur_h: 0,
        scale: 1,
        outputs: vec![],
        repeat_rate: 25,
        repeat_delay: 400,
        repeating: None,
        pointer_over_row: false,
        last_click: None,
        settings_error: None,
        cfg,
        compositor: None,
        shm: None,
        shell: None,
        seat: None,
        kb: None,
        ptr: None,
        surface: None,
        layer: None,
        buffer: None,
        ctx: xkb::Context::new(xkb::CONTEXT_NO_FLAGS),
        xkb: None,
        clip: clip::History::new(0),
        dc_manager: None,
        dc_device: None,
        offers: Default::default(),
        reads: vec![],
        own_source: None,
    };
    app.clip = if app.cfg.clipboard_persist {
        clip::History::load(app.cfg.clipboard_max)
    } else {
        clip::History::new(app.cfg.clipboard_max)
    };
    queue.roundtrip(&mut app).unwrap();
    queue.roundtrip(&mut app).unwrap();
    app.init_clip(&qh);
    if app.shell.is_none() {
        eprintln!("zl: the compositor has no wlr-layer-shell support");
        std::process::exit(1);
    }
    loop {
        queue.dispatch_pending(&mut app).unwrap();
        queue.flush().unwrap();
        let Some(guard) = queue.prepare_read() else {
            continue;
        };
        let mut fds = vec![
            libc::pollfd {
                fd: guard.connection_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: listener.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        for r in &app.reads {
            fds.push(libc::pollfd {
                fd: r.file.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            });
        }
        let mut timeout = match app.repeating {
            Some((_, at)) => at.saturating_duration_since(Instant::now()).as_millis() as i32,
            None => -1,
        };
        if !app.reads.is_empty() {
            timeout = if timeout < 0 { 500 } else { timeout.min(500) };
        }
        unsafe {
            libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout);
        }
        app.finish_reads(&qh);
        if fds[0].revents & libc::POLLIN != 0 {
            let _ = guard.read();
        } else {
            drop(guard);
        }
        if let Some((key, at)) = app.repeating {
            if Instant::now() >= at {
                let interval = Duration::from_millis((1000 / app.repeat_rate.max(1)) as u64);
                if app.key(key, &qh) {
                    app.repeating = Some((key, Instant::now() + interval));
                } else {
                    app.repeating = None;
                }
            }
        }
        if fds[1].revents & libc::POLLIN != 0 {
            if let Ok((mut s, _)) = listener.accept() {
                s.set_nonblocking(false).ok();
                s.set_read_timeout(Some(Duration::from_millis(200))).ok();
                let mut b = String::new();
                let _ = s.read_to_string(&mut b);
                match b.as_str() {
                    "show" => app.show(&qh),
                    "hide" => app.hide(),
                    "toggle" => {
                        if app.shown {
                            app.hide()
                        } else {
                            app.show(&qh)
                        }
                    }
                    "clear-clipboard" => {
                        app.clip.clear();
                        if app.cfg.clipboard_persist {
                            app.clip.save();
                        }
                        app.refresh();
                        app.redraw(&qh);
                    }
                    "quit" => {
                        let _ = std::fs::remove_file(sock());
                        return;
                    }
                    _ => {}
                }
            }
        }
    }
}
