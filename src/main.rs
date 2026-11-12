//! zl: a small Wayland launcher. One daemon, a unix socket, a layer-shell panel drawn on the CPU.
mod actions_ui;
mod clip;
mod config;
#[allow(dead_code)]
mod dates;
#[allow(dead_code)]
mod hist;
mod icons;
mod items;
mod keys;
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
    files: items::FileIndex,
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
    /// Last pointer position on the panel, in surface pixels. Hover never moves the selection.
    pointer: Option<(f64, f64)>,
    last_click: Option<(usize, Instant)>,
    settings_error: Option<String>,
    hist: hist::Hist,
    panel: Option<(String, Vec<Item>, usize)>,
    panel_filter: String,
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
    fn columns(&self) -> usize {
        if self.query.starts_with(':') {
            ui::emoji_columns(self.cfg.width)
        } else {
            1
        }
    }

    fn emoji_rows(&self) -> usize {
        ui::emoji_rows(self.items.len(), self.cfg.width, self.cfg.max_rows)
    }

    fn vp(&self) -> f32 {
        if self.query.starts_with(':') {
            self.emoji_rows() as f32 * ui::EMOJI_CELL
        } else {
            ui::viewport(&self.items, self.cfg.max_rows)
        }
    }

    fn panel_height(&self) -> u32 {
        if self.query.starts_with(':') {
            ui::emoji_panel_height(self.emoji_rows())
        } else {
            ui::panel_height(&self.items, self.cfg.max_rows)
        }
    }

    fn refresh(&mut self) {
        if self.query.starts_with('/') {
            self.files
                .load(&self.cfg.file_roots, &self.cfg.file_excludes);
        }
        let s = Search {
            cfg: &self.cfg,
            apps: &self.apps,
            files: self.files.files(),
            history: &self.clip,
        };
        self.items = s.run(&self.query);
        if !self.query.starts_with([':', ';']) && !self.query.starts_with("settings") {
            self.items.sort_by_key(|it| {
                let key = items::key_of(it);
                (
                    self.hist.pin_order(&key).unwrap_or(usize::MAX),
                    std::cmp::Reverse(self.hist.bonus(&key)),
                )
            });
        }
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
                    best: false,
                },
            );
        }
    }

    fn ensure_visible(&mut self) {
        if self.query.starts_with(':') {
            let columns = self.columns();
            let rows = self.emoji_rows();
            let selected_row = self.sel / columns;
            let mut top_row = self.top / columns;
            if selected_row < top_row {
                top_row = selected_row;
            }
            if selected_row >= top_row + rows {
                top_row = selected_row + 1 - rows;
            }
            self.top =
                top_row.min(self.items.len().div_ceil(columns).saturating_sub(rows)) * columns;
        } else {
            self.top = ui::scroll_to(&self.items, self.sel, self.top, self.vp());
        }
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
        self.cur_h = self.panel_height();
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
        self.files.clear();
        self.repeating = None;
        self.close_panel();
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
        let h = self.panel_height();
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
        let vp = self.vp();
        let (visible, icon_size): (Vec<usize>, u32) = if self.query.starts_with(':') {
            (
                (self.top..(self.top + self.emoji_rows() * self.columns()).min(self.items.len()))
                    .collect(),
                ui::emoji_icon_px(s),
            )
        } else {
            (
                ui::slots(&self.items, self.top, vp)
                    .iter()
                    .filter_map(|x| x.item)
                    .collect(),
                ui::icon_px(s),
            )
        };
        for i in visible {
            if let Some(n) = &self.items[i].icon {
                self.icons.ensure(n, icon_size);
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
            vp,
            theme: self.theme,
            w: self.cfg.width,
            h,
            scale: s,
            font_size: self.cfg.font_size,
        };
        ui::draw(&mut cv, &self.font, &v);
        if let Some((title, actions, sel)) = &self.panel {
            actions_ui::draw(
                &mut cv,
                &self.font,
                &v,
                &actions_ui::PanelView {
                    title,
                    actions,
                    sel: *sel,
                    filter: &self.panel_filter,
                },
            );
        }
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

    fn change_setting(&mut self, key: &str, dir: i32, qh: &QueueHandle<App>) {
        if !settings::step(&mut self.cfg, key, dir) {
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

    fn activate(&mut self, qh: &QueueHandle<App>) {
        let picked = if self.panel.is_some() {
            let vis = self.panel_vis();
            self.panel
                .as_ref()
                .and_then(|p| vis.get(p.2).map(|&i| p.1[i].clone()))
        } else {
            self.items.get(self.sel).cloned()
        };
        let Some(it) = picked else { return };
        self.close_panel();
        if matches!(
            it.action,
            Action::App(_) | Action::Clip(_) | Action::ClipImg(_)
        ) {
            let key = if it.tag == "Action" {
                self.items
                    .get(self.sel)
                    .map(items::key_of)
                    .unwrap_or_default()
            } else {
                items::key_of(&it)
            };
            self.hist.record(&key);
        }
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
            Action::Open(p) if it.tag == "File" && std::path::Path::new(&p).is_dir() => {
                // Enter on a folder drills into it.
                let home = std::env::var("HOME").unwrap_or_default();
                let rel = p.strip_prefix(&format!("{home}/")).unwrap_or(&p);
                self.query = format!("/{rel}/");
                self.sel = 0;
                self.top = 0;
                self.refresh();
                self.redraw(qh);
            }
            Action::Open(p) => {
                self.hide();
                let _ = std::process::Command::new("xdg-open")
                    .arg(p)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
            Action::Nothing => {}
            Action::View(view) => {
                self.query = view;
                self.refresh();
                self.redraw(qh);
            }
            Action::Setting(key) => self.change_setting(&key, 1, qh),
            Action::Exclude(name) => {
                if !settings::exclude(&mut self.cfg, &name) {
                    self.settings_error = Some("Could not save exclusions".into());
                }
                let sel = self.sel;
                self.refresh();
                self.sel = sel.min(self.items.len().saturating_sub(1));
                self.redraw(qh);
            }
            Action::PinToggle(key) => {
                self.hist.toggle_pin(&key);
                self.refresh();
                self.redraw(qh);
            }
            Action::ResetRank(key) => {
                self.hist.reset(&key);
                self.refresh();
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

    /// The action row (index into the filtered list) under the surface point, if the point is on one.
    fn panel_row_at(&self, x: f64, y: f64) -> Option<usize> {
        let (_, _, selected) = self.panel.as_ref()?;
        let g = actions_ui::panel_layout(
            self.cfg.width as f32,
            self.vp(),
            self.panel_vis().len(),
            *selected,
            1.0,
        );
        let ry = y as f32 - g.y - actions_ui::PANEL_HEAD;
        let row = (ry / ui::ROW) as usize;
        let inside = x as f32 >= g.x && (x as f32) < g.x + g.w && ry >= 0.0 && row < g.cap;
        inside.then_some(g.off + row)
    }

    fn close_panel(&mut self) {
        self.panel = None;
        self.panel_filter.clear();
    }

    fn panel_vis(&self) -> Vec<usize> {
        let Some((_, acts, _)) = &self.panel else {
            return Vec::new();
        };
        acts.iter()
            .enumerate()
            .filter(|(_, a)| a.title.to_lowercase().contains(&self.panel_filter))
            .map(|(i, _)| i)
            .collect()
    }

    fn open_panel(&mut self) {
        let Some(it) = self.items.get(self.sel) else {
            return;
        };
        let acts = items::actions_for(it, self.hist.is_pinned(&items::key_of(it)), &self.cfg);
        self.panel = Some((it.title.clone(), acts, 0));
        self.panel_filter.clear();
    }

    fn key(&mut self, key: u32, qh: &QueueHandle<App>) -> bool {
        use xkbcommon::xkb::keysyms as k;
        let Some(st) = &self.xkb else { return false };
        let code = xkb::Keycode::new(key + 8);
        let sym = st.key_get_one_sym(code).raw();
        let utf8 = st.key_get_utf8(code);
        let ctrl = st.mod_name_is_active(xkb::MOD_NAME_CTRL, xkb::STATE_MODS_EFFECTIVE);
        let mut repeatable = true;
        if self.panel.is_some() {
            let n = self.panel_vis().len();
            match sym {
                k::KEY_Escape => self.close_panel(),
                k::KEY_Return | k::KEY_KP_Enter => {
                    self.activate(qh);
                    return false;
                }
                k::KEY_Down | k::KEY_Tab => {
                    if let Some(p) = &mut self.panel {
                        p.2 = (p.2 + 1).min(n.saturating_sub(1));
                    }
                }
                k::KEY_Up | k::KEY_ISO_Left_Tab => {
                    if let Some(p) = &mut self.panel {
                        p.2 = p.2.saturating_sub(1);
                    }
                }
                k::KEY_BackSpace => {
                    self.panel_filter.pop();
                    if let Some(p) = &mut self.panel {
                        p.2 = 0;
                    }
                }
                _ if ctrl && sym == k::KEY_b => self.close_panel(),
                _ if !ctrl && !utf8.is_empty() && !utf8.chars().any(char::is_control) => {
                    self.panel_filter.push_str(&utf8.to_lowercase());
                    if let Some(p) = &mut self.panel {
                        p.2 = 0;
                    }
                }
                _ => repeatable = false,
            }
            self.redraw(qh);
            return repeatable;
        }
        if ctrl && sym == k::KEY_b {
            self.open_panel();
            self.redraw(qh);
            return false;
        }
        let (columns, grid) = (self.columns(), self.query.starts_with(':'));
        let selected = self.items.get(self.sel).map(|i| i.action.clone());
        let effect = keys::handle(
            &mut keys::KeyState {
                query: &mut self.query,
                sel: &mut self.sel,
                len: self.items.len(),
                rows: self.cfg.max_rows,
                columns,
                grid,
                pop_on_backspace: self.cfg.pop_on_backspace,
                close_on_escape: self.cfg.escape_key_behavior == "close_window",
                selected: selected.as_ref(),
            },
            &keys::KeyIn {
                sym,
                utf8: &utf8,
                ctrl,
            },
        );
        let repeat = effect.repeats();
        match effect {
            keys::Effect::Ignore => return false,
            keys::Effect::Hide => {
                self.hide();
                return false;
            }
            keys::Effect::Activate => {
                self.activate(qh);
                return false;
            }
            keys::Effect::Edited => self.refresh(),
            keys::Effect::Moved => {}
            keys::Effect::Step(name, dir) => {
                self.change_setting(&name, dir, qh);
                return true;
            }
            keys::Effect::DeleteClip => {
                if let Some(action) = selected {
                    if self.clip.remove(&action) && self.cfg.clipboard_persist {
                        self.clip.save();
                    }
                }
                self.refresh();
            }
        }
        self.redraw(qh);
        repeat
    }

    fn row_at(&self, x: f64, y: f64) -> Option<usize> {
        if self.query.starts_with(':') {
            return ui::emoji_hit(
                self.cfg.width,
                self.emoji_rows(),
                self.top,
                self.items.len(),
                x as f32,
                y as f32,
            );
        }
        ui::item_at(
            &self.items,
            self.top,
            self.vp(),
            y as f32 - ui::HEAD - ui::LIST_PAD,
        )
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
            Event::Leave { .. } => {
                // Losing keyboard focus hides the panel, like zlaunch.
                a.repeating = None;
                if a.shown {
                    a.hide();
                }
            }
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
            Event::Motion {
                surface_x,
                surface_y,
                ..
            }
            | Event::Enter {
                surface_x,
                surface_y,
                ..
            } => {
                a.pointer = Some((surface_x, surface_y));
                if a.panel.is_some() {
                    if let Some(row) = a.panel_row_at(surface_x, surface_y) {
                        if let Some(p) = &mut a.panel {
                            p.2 = row;
                        }
                        a.redraw(qh);
                    }
                }
            }
            Event::Leave { .. } => a.pointer = None,
            Event::Button {
                button: 272,
                state: WEnum::Value(ButtonState::Pressed),
                ..
            } => {
                if a.shown && a.panel.is_some() {
                    // A click on an action runs it; a click anywhere else only closes the panel.
                    match a.pointer.and_then(|(x, y)| a.panel_row_at(x, y)) {
                        Some(row) => {
                            if let Some(p) = &mut a.panel {
                                p.2 = row;
                            }
                            a.activate(qh);
                        }
                        None => {
                            a.close_panel();
                            a.redraw(qh);
                        }
                    }
                    return;
                }
                // A click picks the row under the pointer and runs it, like zlaunch.
                let row = a.pointer.and_then(|(x, y)| a.row_at(x, y));
                if let (true, Some(i)) = (a.shown, row) {
                    a.sel = i;
                    let now = Instant::now();
                    let double = a.last_click.is_some_and(|(j, t)| {
                        j == i && now.duration_since(t) < Duration::from_millis(400)
                    });
                    if a.cfg.single_click || double {
                        a.last_click = None;
                        a.activate(qh);
                    } else {
                        a.last_click = Some((i, now));
                        a.redraw(qh);
                    }
                }
            }
            Event::Axis {
                axis: WEnum::Value(Axis::VerticalScroll),
                value,
                ..
            } => {
                if a.panel.is_some() {
                    let n = a.panel_vis().len();
                    if let Some(p) = &mut a.panel {
                        p.2 = (p.2 as isize + if value > 0.0 { 1 } else { -1 })
                            .clamp(0, n.saturating_sub(1) as isize)
                            as usize;
                    }
                    a.redraw(qh);
                    return;
                }
                if a.query.starts_with(':') {
                    let columns = a.columns();
                    let rows = a.emoji_rows();
                    let max_top = a.items.len().div_ceil(columns).saturating_sub(rows);
                    let step = if value > 0.0 { 1isize } else { -1 };
                    a.top = ((a.top / columns) as isize + step).clamp(0, max_top as isize) as usize
                        * columns;
                    a.sel = a
                        .sel
                        .clamp(a.top, a.top + rows * columns - 1)
                        .min(a.items.len().saturating_sub(1));
                } else {
                    let vp = a.vp();
                    let max_top = ui::max_top(&a.items, vp);
                    let step = if value > 0.0 { 1isize } else { -1 };
                    a.top = (a.top as isize + step).clamp(0, max_top as isize) as usize;
                    let shown: Vec<usize> = ui::slots(&a.items, a.top, vp)
                        .iter()
                        .filter_map(|x| x.item)
                        .collect();
                    if let (Some(&f), Some(&l)) = (shown.first(), shown.last()) {
                        a.sel = a.sel.clamp(f, l);
                    }
                }
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
        files: items::FileIndex::default(),
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
        pointer: None,
        last_click: None,
        settings_error: None,
        hist: hist::Hist::load(),
        panel: None,
        panel_filter: String::new(),
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
