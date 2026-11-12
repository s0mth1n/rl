//! zl: a small Wayland launcher. One daemon, a unix socket, a layer-shell panel drawn on the CPU.
mod app_cache;
mod app_watch;
mod calculator;
mod cli;
mod clip;
mod clipboard_ui;
mod config;
mod control_io;
#[allow(dead_code)]
mod dates;
#[allow(dead_code)]
mod hist;
mod hyprland_blur;
mod icons;
mod ime;
mod input;
mod input_menu;
mod items;
mod keys;
mod launch;
mod modes;
mod scorer;
mod scroll;
mod socket_path;
mod system_actions;
#[cfg(test)]
mod testutil;
mod ui;
mod web_search;
mod windows;

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
    wl_subcompositor::WlSubcompositor,
    wl_subsurface::WlSubsurface,
    wl_surface::WlSurface,
};
use wayland_client::Proxy;
use wayland_client::{delegate_noop, Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    zwp_text_input_v3::{self, ZwpTextInputV3},
};
use wayland_protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};
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

struct Backdrop {
    surface: WlSurface,
    panel: WlSubsurface,
    viewport: Option<WpViewport>,
    buffer: Option<WlBuffer>,
    width: u32,
    height: u32,
}

fn backdrop_buffer_size(width: u32, height: u32, viewport: bool) -> Option<(u32, u32, usize)> {
    if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return None;
    }
    let (w, h) = if viewport { (1, 1) } else { (width, height) };
    let bytes = (w as usize).checked_mul(h as usize)?.checked_mul(4)?;
    (bytes <= 256 * 1024 * 1024).then_some((w, h, bytes))
}

fn explicit_window_size(cfg: &Config, panel_height: u32) -> Option<(u32, u32)> {
    let (w, h) = cfg.window_size?;
    if !cfg.enable_backdrop || !w.is_finite() || !h.is_finite() {
        return None;
    }
    Some((
        w.max(cfg.width as f32).ceil().min(i32::MAX as f32) as u32,
        h.max(panel_height as f32).ceil().min(i32::MAX as f32) as u32,
    ))
}

fn default_backdrop_size() -> (u32, u32) {
    let kwin = std::env::var_os("KDE_SESSION_VERSION").is_some()
        && std::process::Command::new("timeout")
            .args([
                "1",
                "gdbus",
                "call",
                "--session",
                "--dest",
                "org.kde.KWin",
                "--object-path",
                "/KWin",
                "--method",
                "org.kde.KWin.supportInformation",
            ])
            .output()
            .is_ok_and(|out| out.status.success());
    if kwin {
        (1920, 1080)
    } else {
        (5680, 4320)
    }
}

fn panel_position(width: u32, height: u32, panel_width: u32, panel_height: u32) -> (i32, i32) {
    (
        ((width as i64 - panel_width as i64) / 2) as i32,
        ((height as i64 - panel_height as i64) / 2) as i32,
    )
}

struct Out {
    obj: WlOutput,
    name: String,
    scale: u32,
}

struct App {
    cfg: Config,
    modes: modes::State,
    theme: config::Theme,
    saved_theme_name: String,
    icons: icons::Cache,
    font: Font<'static>,
    font_size: f32,
    mono_font_family: Option<String>,
    bold_font: Option<Font<'static>>,
    apps: Vec<DesktopApp>,
    launched: Vec<std::process::Child>,
    windows: Vec<windows::Window>,
    items: Vec<Item>,
    query: String,
    edit: input::Edit,
    ime: ime::State,
    text_manager: Option<ZwpTextInputManagerV3>,
    text_input: Option<ZwpTextInputV3>,
    text_focus: Option<WlSurface>,
    text_commits: u32,
    text_waiting: bool,
    text_snapshot: Option<ime::Snapshot>,
    caret_visible: bool,
    caret_next: Instant,
    sel: usize,
    top: usize,
    scroll_remainder: f32,
    scrollbar: scroll::State,
    shown: bool,
    configured: bool,
    cur_h: u32,
    scale: u32,
    outputs: Vec<Out>,
    active_output: Option<WlOutput>,
    repeat_rate: i32,
    repeat_delay: i32,
    repeating: Option<(u32, Instant)>,
    /// Last pointer position on the panel, in surface pixels. Hover never moves the selection.
    pointer: Option<(f64, f64)>,
    pointer_on_backdrop: bool,
    last_click: Option<(usize, Instant)>,
    /// A discrete wheel step arrived in this frame; the release scrolls 1.3x for those.
    wheel_discrete: bool,
    /// A sub-view was entered from the combined list, so show a back arrow.
    origin: Option<String>,
    input_drag: bool,
    input_menu: Option<input_menu::Menu>,
    input_click: Option<(Instant, u8, f64, f64)>,
    settings_error: Option<String>,
    hist: hist::Hist,
    /// A picker selection is previewing a theme that is not saved yet.
    previewing: bool,
    theme_catalog: Option<Vec<Item>>,
    theme_palettes: std::collections::HashMap<String, config::Theme>,
    compositor: Option<WlCompositor>,
    subcompositor: Option<WlSubcompositor>,
    viewporter: Option<WpViewporter>,
    backdrop: Option<Backdrop>,
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
    compose: Option<xkb::compose::State>,
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
    files: bool,
}

impl App {
    fn view_query(&self) -> &str {
        if self.modes.current().routes_prefixes() {
            &self.query
        } else {
            ""
        }
    }

    fn layout(&self) -> ui::Layout {
        let mut layout = ui::Layout::new(&self.theme.metrics);
        if self.view_query().starts_with(';') {
            layout = ui::Layout::clipboard(&self.theme.metrics);
        }
        layout
    }

    fn columns(&self) -> usize {
        if self.view_query().starts_with(':') {
            self.layout().emoji_columns(self.cfg.width)
        } else {
            1
        }
    }

    fn vp(&self) -> f32 {
        if self.cfg.launcher_size.is_none() {
            return ui::LIST_VP;
        }
        (self.panel_height() as f32 - ui::HEAD - ui::LIST_PAD * 2.0).max(0.0)
    }

    /// zlaunch fixes every panel at `launcher_size` (default 600x400), so the
    /// panel height never follows the content (zlaunch render.rs applies the
    /// configured size unconditionally).
    fn panel_height(&self) -> u32 {
        self.cfg
            .launcher_size
            .map(|_| self.cfg.panel_size().1)
            .unwrap_or(ui::PANEL_HEIGHT)
    }

    fn refresh(&mut self) {
        // Typing away the sub-view prefix also leaves the sub-view.
        if self.origin.is_some() && !self.view_query().starts_with([':', ';', '#']) {
            self.origin = None;
        }
        self.edit.literal_prefix = !self.modes.current().routes_prefixes();
        self.edit.sync(&self.query);
        let mut cfg = self.cfg.clone();
        if let modes::Mode::Module(module) = self.modes.current() {
            cfg.combined_modules = Some(vec![module]);
            cfg.disabled_modules = None;
        }
        let s = Search {
            cfg: &cfg,
            apps: &self.apps,
            windows: &self.windows,
            history: &self.clip,
        };
        if let Some(filter) = self
            .query
            .strip_prefix('#')
            .filter(|_| self.modes.current().routes_prefixes())
        {
            let catalog = self
                .theme_catalog
                .get_or_insert_with(|| items::theme_items(&self.cfg, ""));
            if self.theme_palettes.is_empty() {
                self.theme_palettes = catalog
                    .iter()
                    .filter_map(|item| {
                        config::load_theme(&item.title).map(|theme| (item.title.clone(), theme))
                    })
                    .collect();
            }
            let filter = filter.to_lowercase();
            self.items = catalog
                .iter()
                .filter(|item| {
                    item.title.to_lowercase().contains(&filter)
                        || item.sub.to_lowercase().contains(&filter)
                })
                .cloned()
                .collect();
        } else {
            self.theme_catalog = None;
            self.theme_palettes.clear();
            self.items = if self.modes.current().routes_prefixes() {
                s.run(&self.query)
            } else {
                s.run_mode(&self.query, false)
            };
        }
        self.sel = 0;
        self.top = 0;
        self.scroll_remainder = 0.0;
        self.scrollbar = scroll::State::default();
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
        let (start, height) = if self.view_query().starts_with(':') {
            (
                (self.sel / self.columns()) as f32 * self.layout().cell,
                self.layout().cell,
            )
        } else {
            (
                self.layout().scroll_item_y(
                    &self.items,
                    self.sel,
                    self.view_query().starts_with(';'),
                ),
                self.layout().row,
            )
        };
        let current = self.scrollbar_geometry().map_or(0.0, |g| {
            if self.view_query().starts_with(':') {
                (self.top / self.columns()) as f32 * self.layout().cell + self.scroll_remainder
            } else {
                self.layout().scroll_offset(
                    &self.items,
                    self.top,
                    self.view_query().starts_with(';'),
                ) + self.scroll_remainder
            }
            .min(g.content - g.viewport)
        });
        if self.sel == 0 || start < current {
            self.scroll_to_offset(start);
        } else if start + height > current + self.vp() {
            self.scroll_to_offset(start + height - self.vp());
        }
    }

    fn pause_caret(&mut self) {
        self.caret_visible = true;
        self.caret_next = Instant::now() + Duration::from_millis(300);
    }

    /// Open a view by its query prefix after `show`, as `show -m emojis` does.
    /// Mode names from `show -m`; empty goes back to the configured default modes.
    fn set_modes(&mut self, names: &str) {
        let names: Vec<String> = names
            .split(',')
            .filter(|n| !n.is_empty())
            .map(String::from)
            .collect();
        self.modes = if names.is_empty() {
            modes::State::new(self.cfg.default_modes.as_deref())
        } else {
            modes::State::new(Some(&names))
        };
    }

    fn setup_text_input(&mut self, qh: &QueueHandle<App>) {
        if self.text_input.is_none() {
            if let (Some(manager), Some(seat)) = (&self.text_manager, &self.seat) {
                self.text_input = Some(manager.get_text_input(seat, qh, ()));
            }
        }
    }

    fn update_text_input(&mut self, from_ime: bool) {
        if !self.shown || self.text_focus.is_none() || self.text_waiting {
            return;
        }
        let Some(input) = &self.text_input else {
            return;
        };
        let surrounding = ime::surrounding_with_prefix(
            &self.query,
            self.edit.caret,
            self.edit.anchor.unwrap_or(self.edit.caret),
            self.edit.prefix_len(&self.query),
        );
        let display = self.ime.display(&self.query, &self.edit);
        let (shown, caret) = display
            .as_ref()
            .map_or((self.query.as_str(), self.edit.caret), |d| {
                (d.text.as_str(), d.caret)
            });
        let (mut x, mut y, width, height) =
            ui::input_cursor_rectangle(&self.font, self.view_query(), shown, caret, self.cfg.width);
        if self
            .backdrop
            .as_ref()
            .is_some_and(|b| self.text_focus.as_ref() == Some(&b.surface))
        {
            let b = self.backdrop.as_ref().unwrap();
            let (px, py) = panel_position(b.width, b.height, self.cfg.width, self.cur_h);
            x += px;
            y += py;
        }
        let rect = (x, y, width, height);
        let snapshot = ime::Snapshot { surrounding, rect };
        if self.text_snapshot.as_ref() == Some(&snapshot) {
            return;
        }
        if self
            .text_snapshot
            .as_ref()
            .is_some_and(|old| old.surrounding.is_some() != snapshot.surrounding.is_some())
        {
            input.enable();
        }
        if let Some((text, cursor, anchor)) = &snapshot.surrounding {
            input.set_surrounding_text(text.clone(), *cursor, *anchor);
        }
        input.set_text_change_cause(if from_ime {
            zwp_text_input_v3::ChangeCause::InputMethod
        } else {
            zwp_text_input_v3::ChangeCause::Other
        });
        input.set_content_type(
            zwp_text_input_v3::ContentHint::None,
            zwp_text_input_v3::ContentPurpose::Normal,
        );
        input.set_cursor_rectangle(x, y, width, height);
        input.commit();
        self.text_commits = self.text_commits.wrapping_add(1);
        self.text_snapshot = Some(snapshot);
    }

    fn show(&mut self, qh: &QueueHandle<App>) {
        self.pause_caret();
        if self.shown {
            return;
        }
        if self.compositor.is_none() || self.shell.is_none() {
            return;
        }
        self.apps = app_cache::load(&self.cfg);
        self.windows = if self
            .cfg
            .combined_modules()
            .contains(&config::ConfigModule::Windows)
        {
            windows::scan()
        } else {
            vec![]
        };
        // zlaunch uses compositor-provided icon pixels when they exist.
        for window in &self.windows {
            if let Some(png) = &window.icon_data {
                self.icons.set_window_icon(&window.address, png.clone());
            }
        }
        self.last_click = None;
        self.pointer = None;
        self.query = self.modes.current().prefix().into();
        self.origin = None;
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
        self.cur_h = self.panel_height();
        let root = if let Some(sub) = self
            .subcompositor
            .as_ref()
            .filter(|_| self.cfg.enable_backdrop)
        {
            let root = c.create_surface(qh, ());
            let panel = sub.get_subsurface(&s, &root, qh, ());
            panel.set_desync();
            let viewport = self
                .viewporter
                .as_ref()
                .map(|v| v.get_viewport(&root, qh, ()));
            self.backdrop = Some(Backdrop {
                surface: root.clone(),
                panel,
                viewport,
                buffer: None,
                width: 0,
                height: 0,
            });
            root
        } else {
            s.clone()
        };
        let layer = match self.cfg.layer_shell_layer {
            config::SurfaceLayer::Top => Layer::Top,
            config::SurfaceLayer::Overlay => Layer::Overlay,
        };
        let l = sh.get_layer_surface(&root, out, layer, "zl".into(), qh, ());
        if self.backdrop.is_some() {
            let (width, height) =
                explicit_window_size(&self.cfg, self.cur_h).unwrap_or_else(|| {
                    if self.backdrop.as_ref().is_some_and(|b| b.viewport.is_some()) {
                        default_backdrop_size()
                    } else {
                        (0, 0)
                    }
                });
            l.set_size(width, height);
            l.set_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right);
            l.set_exclusive_zone(-1);
        } else {
            l.set_size(self.cfg.width, self.cur_h);
            l.set_anchor(Anchor::empty());
        }
        l.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        root.commit();
        self.pointer_on_backdrop = false;
        self.pointer = None;
        self.surface = Some(s);
        self.layer = Some(l);
        self.shown = true;
        self.configured = false;
    }

    fn hide(&mut self) {
        self.active_output = None;
        self.theme_catalog = None;
        self.theme_palettes.clear();
        self.input_menu = None;
        self.ime.clear();
        self.text_snapshot = None;
        if self.text_focus.take().is_some() {
            if let Some(input) = &self.text_input {
                input.disable();
                input.commit();
                self.text_commits = self.text_commits.wrapping_add(1);
            }
        }
        self.repeating = None;

        self.pointer_on_backdrop = false;
        self.pointer = None;
        self.scrollbar = scroll::State::default();
        self.input_drag = false;
        self.input_click = None;
        if let Some(b) = self.backdrop.as_ref() {
            b.panel.destroy();
        }
        // Dismissing the panel cancels an unsaved theme preview.
        if self.previewing {
            self.theme = self.cfg.theme();
            self.previewing = false;
        }
        if let Some(l) = self.layer.take() {
            l.destroy();
        }
        if let Some(b) = self.buffer.take() {
            b.destroy();
        }
        if let Some(s) = self.surface.take() {
            s.destroy();
        }
        if let Some(b) = self.backdrop.take() {
            if let Some(v) = b.viewport {
                v.destroy();
            }
            if let Some(buffer) = b.buffer {
                buffer.destroy();
            }
            b.surface.destroy();
        }
        self.shown = false;
        self.configured = false;
    }

    fn redraw(&mut self, qh: &QueueHandle<App>) {
        if !self.shown || !self.configured {
            return;
        }
        self.sync_theme_preview();
        let h = self.panel_height();
        if h != self.cur_h {
            self.cur_h = h;
            if self.backdrop.is_none() {
                if let Some(l) = &self.layer {
                    l.set_size(self.cfg.width, h);
                }
            }
        }
        if self.shm.is_none() || self.surface.is_none() {
            return;
        }
        let s = self.scale;
        let vp = self.vp();
        let (visible, icon_size): (Vec<usize>, u32) = if self.view_query().starts_with(':') {
            (
                (self.top
                    ..(self.top
                        + ((self.vp() + self.scroll_remainder) / self.layout().cell).ceil()
                            as usize
                            * self.columns())
                    .min(self.items.len()))
                    .collect(),
                (self.theme.metrics.emoji_font_size * s as f32)
                    .round()
                    .max(1.0) as u32,
            )
        } else {
            (
                self.layout()
                    .slots(&self.items, self.top, vp + self.scroll_remainder)
                    .iter()
                    .filter_map(|x| x.item)
                    .collect(),
                ui::icon_px(&self.theme, s),
            )
        };
        for i in visible {
            if let Some(n) = &self.items[i].icon {
                self.icons.ensure(n, icon_size);
            }
        }
        self.icons.ensure_input(s);
        if self.view_query().starts_with(';') && self.origin.is_none() {
            self.icons.ensure("phosphor:copy", 16 * s);
        }
        if let Some(b) = &self.backdrop {
            let (x, y) = panel_position(b.width, b.height, self.cfg.width, h);
            b.panel.set_position(x, y);
            b.surface.commit();
        }
        let Some((physical_w, physical_h, _)) = self
            .cfg
            .width
            .checked_mul(s)
            .zip(h.checked_mul(s))
            .and_then(|(w, h)| backdrop_buffer_size(w, h, false))
        else {
            eprintln!("zl: panel buffer exceeds 256 MiB or has invalid dimensions");
            self.hide();
            return;
        };
        self.update_text_input(false);
        let Some(surf) = &self.surface else { return };
        let mut cv = ui::Canvas::new(physical_w, physical_h);
        let composition = self.ime.display(&self.query, &self.edit);
        let (caret, anchor, visible) = composition.as_ref().map_or(
            (self.edit.caret, self.edit.anchor, self.caret_visible),
            |d| (d.caret, d.anchor, !d.hidden),
        );
        let v = ui::View {
            theme_palettes: Some(&self.theme_palettes),
            icons: &self.icons,
            query: &self.query,
            literal_prefix: !self.modes.current().routes_prefixes(),
            placeholder: Some(self.modes.current().placeholder()),
            input_text: composition.as_ref().map(|d| d.text.as_str()),
            marked: composition.as_ref().map(|d| d.marked.clone()),
            input: Some((caret, anchor, visible)),
            items: &self.items,
            sel: self.sel,
            top: self.top,
            scroll_remainder: self.scroll_remainder,
            vp,
            theme: self.theme,
            w: self.cfg.width,
            h,
            scale: s,
            font_size: self.font_size,
            mono_font_family: self.mono_font_family.as_deref(),
            bold_font: self.bold_font.as_ref(),
            back: self.origin.is_some(),
            scrollbar: self.scrollbar.style(),
        };
        if self.view_query().starts_with(';') {
            clipboard_ui::draw(&mut cv, &self.font, &v, &self.clip);
        } else {
            ui::draw(&mut cv, &self.font, &v);
        }

        if let Some(menu) = &self.input_menu {
            menu.draw(&mut cv, &self.font, self.scale);
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

    fn configure_backdrop(&mut self, width: u32, height: u32, qh: &QueueHandle<App>) -> bool {
        let Some(b) = self.backdrop.as_mut() else {
            return true;
        };
        let Some(shm) = &self.shm else { return false };
        let Some((w, h, size)) = backdrop_buffer_size(width, height, b.viewport.is_some()) else {
            return false;
        };
        let fd = unsafe { libc::memfd_create(c"zl-backdrop".as_ptr(), 0) };
        if fd < 0 {
            return false;
        }
        let file = unsafe { std::fs::File::from_raw_fd(fd) };
        if file.set_len(size as u64).is_err() {
            return false;
        }
        let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
        let buffer = pool.create_buffer(
            0,
            w as i32,
            h as i32,
            (w * 4) as i32,
            Format::Argb8888,
            qh,
            (),
        );
        pool.destroy();
        if let Some(v) = &b.viewport {
            v.set_destination(width as i32, height as i32);
        }
        b.width = width;
        b.height = height;
        b.surface.attach(Some(&buffer), 0, 0);
        b.surface.damage_buffer(0, 0, w as i32, h as i32);
        if let Some(old) = b.buffer.replace(buffer) {
            old.destroy();
        }
        true
    }

    /// Live theme preview: in the picker the selected row recolours the panel, like
    /// zlaunch's theme mode. Leaving the picker without Enter restores the saved theme.
    fn sync_theme_preview(&mut self) {
        let picked = if items::theme_view(self.view_query()) {
            self.items.get(self.sel).and_then(|it| match &it.action {
                Action::Theme(name) => Some(name.clone()),
                _ => None,
            })
        } else {
            None
        };
        match picked {
            Some(name) => {
                self.theme = self
                    .theme_palettes
                    .get(&name)
                    .copied()
                    .unwrap_or_else(|| config::theme(&name));
                if !self.cfg.enable_transparency {
                    self.theme.bg_alpha = 1.0;
                }
                self.previewing = true;
            }
            None if self.previewing && !items::theme_view(self.view_query()) => {
                self.theme = self.cfg.theme();
                self.previewing = false;
            }
            _ => {}
        }
    }

    fn activate(&mut self, qh: &QueueHandle<App>) {
        let picked = self.items.get(self.sel).cloned();
        let Some(it) = picked else {
            if items::theme_view(self.view_query()) {
                self.query.clear();
                self.refresh();
                self.redraw(qh);
            }
            return;
        };
        if matches!(
            it.action,
            Action::App(_) | Action::Clip(..) | Action::ClipFiles(..) | Action::ClipImg(..)
        ) {
            let key = items::key_of(&it);
            self.hist.record(&key);
        }
        match it.action {
            Action::WebSearch(url) => {
                self.hide();
                match web_search::open(&url) {
                    Ok(child) => self.launched.push(child),
                    Err(error) => eprintln!("zl: failed to open search URL: {error}"),
                }
            }
            Action::System(action) => {
                self.hide();
                match action.execute() {
                    Ok(child) => self.launched.push(child),
                    Err(error) => eprintln!("zl: failed to execute action: {error}"),
                }
            }
            Action::Calculate(result) => {
                self.hide();
                self.set_clipboard(result.text_for_clipboard().as_bytes(), false, qh);
            }
            Action::Window(id) => {
                if let Err(error) = windows::focus(&id) {
                    eprintln!("zl: failed to focus window: {error}");
                }
                self.hide();
            }
            Action::App(i) => {
                self.hide();
                let a = &self.apps[i];
                match launch::application(&a.exec, a.terminal) {
                    Ok(child) => self.launched.push(child),
                    Err(error) => eprintln!("zl: failed to launch application: {error}"),
                }
            }
            Action::Clip(t, _) | Action::ClipFiles(t, _) => {
                self.hide();
                self.set_clipboard(t.as_bytes(), false, qh);
            }
            Action::ClipImg(id, _) => {
                if let Some(image) = self.clip.images.iter().find(|i| i.id == id) {
                    if let Some(png) = image.bytes() {
                        self.hide();
                        self.set_clipboard(&png, true, qh);
                    }
                }
            }
            Action::View(view) => {
                // Entered through the main list: remember the combined query
                // so the back arrow returns to it, as zlaunch's sub-views do.
                if matches!(view.as_str(), ":" | ";" | "#") {
                    self.origin = Some(self.query.clone());
                }
                self.query = view;
                self.refresh();
                self.redraw(qh);
            }
            Action::Theme(name) => {
                config::set_theme(&mut self.cfg, &name);
                self.saved_theme_name = name.clone();
                self.previewing = false;
                self.theme = self.cfg.theme();
                self.query.clear();
                self.refresh();
                self.redraw(qh);
            }
        }
    }

    fn input_command(&mut self, command: usize) {
        let selected = self
            .edit
            .range()
            .map(|r| self.query[r].to_string())
            .unwrap_or_default();
        match command {
            0 | 1 => {
                if !selected.is_empty() && copy_input_text(&selected) && command == 0 {
                    self.edit.replace(&mut self.query, "");
                    self.refresh();
                }
            }
            2 => {
                if let Ok(out) = std::process::Command::new("timeout")
                    .args(["1", "wl-paste", "--no-newline", "--type", "text"])
                    .output()
                {
                    if out.status.success() {
                        let text = input::paste_text(&String::from_utf8_lossy(&out.stdout));
                        self.edit.replace(&mut self.query, &text);
                        self.refresh();
                    }
                }
            }
            3 => self.edit.select_all(&self.query),
            _ => {}
        }
        self.pause_caret();
    }

    fn key(&mut self, key: u32, qh: &QueueHandle<App>) -> bool {
        let Some(st) = &self.xkb else { return false };
        let code = xkb::Keycode::new(key + 8);
        let sym = st.key_get_one_sym(code).raw();
        if matches!(
            sym,
            xkb::keysyms::KEY_Shift_L
                | xkb::keysyms::KEY_Shift_R
                | xkb::keysyms::KEY_Control_L
                | xkb::keysyms::KEY_Control_R
                | xkb::keysyms::KEY_Alt_L
                | xkb::keysyms::KEY_Alt_R
                | xkb::keysyms::KEY_Super_L
                | xkb::keysyms::KEY_Super_R
                | xkb::keysyms::KEY_Meta_L
                | xkb::keysyms::KEY_Meta_R
                | xkb::keysyms::KEY_ISO_Level3_Shift
                | xkb::keysyms::KEY_ISO_Level5_Shift
                | xkb::keysyms::KEY_Caps_Lock
                | xkb::keysyms::KEY_Num_Lock
        ) {
            return false;
        }
        if self.ime.clear() {
            self.refresh();
        }
        self.pause_caret();
        let Some(st) = &self.xkb else { return false };
        let mut utf8 = st.key_get_utf8(code);
        let ctrl = st.mod_name_is_active(xkb::MOD_NAME_CTRL, xkb::STATE_MODS_EFFECTIVE);
        if let Some(menu) = &mut self.input_menu {
            match sym {
                xkb::keysyms::KEY_Escape => {
                    self.input_menu = None;
                }
                xkb::keysyms::KEY_Up => menu.step(true),
                xkb::keysyms::KEY_Down => menu.step(false),
                xkb::keysyms::KEY_Return | xkb::keysyms::KEY_KP_Enter => {
                    let command = menu.selected.filter(|&i| menu.enabled[i]);
                    self.input_menu = None;
                    if let Some(command) = command {
                        self.input_command(command);
                    }
                }
                _ => {}
            }
            self.redraw(qh);
            return false;
        }
        if ctrl && matches!(sym, xkb::keysyms::KEY_Tab | xkb::keysyms::KEY_ISO_Left_Tab) {
            let previous = st.mod_name_is_active(xkb::MOD_NAME_SHIFT, xkb::STATE_MODS_EFFECTIVE);
            if self.modes.cycle(previous) {
                if self.previewing {
                    self.theme = self.cfg.theme();
                    self.previewing = false;
                }
                self.query = self.modes.current().prefix().into();
                self.origin = None;
                self.edit = input::Edit::default();
                self.refresh();
                self.redraw(qh);
            }
            return false;
        }
        if let Some(compose) = self.compose.as_mut() {
            if ctrl || sym == xkb::keysyms::KEY_Escape {
                compose.reset();
            } else {
                compose.feed(sym.into());
                match compose.status() {
                    xkb::compose::Status::Composing => return false,
                    xkb::compose::Status::Composed => {
                        utf8 = compose.utf8().unwrap_or_default();
                        compose.reset();
                    }
                    xkb::compose::Status::Cancelled => {
                        compose.reset();
                        return false;
                    }
                    xkb::compose::Status::Nothing => {}
                }
            }
        }
        let (columns, grid) = (self.columns(), self.view_query().starts_with(':'));
        let selected = self.items.get(self.sel).map(|i| i.action.clone());
        let previous_selection = self.sel;
        // Only zlaunch's own sub-views hide when entered directly; quicklinks
        // keep returning to the list.
        let direct_view = self.origin.is_none()
            && (self.modes.current() != modes::Mode::Combined
                || self.view_query().starts_with([':', ';', '#']));
        let effect = keys::handle(
            &mut keys::KeyState {
                query: &mut self.query,
                edit: &mut self.edit,
                sel: &mut self.sel,
                len: self.items.len(),
                columns,
                grid,
                pop_on_backspace: self.cfg.pop_on_backspace,
                close_on_escape: self.cfg.escape_key_behavior == "close_window",
                direct_view,
            },
            &keys::KeyIn {
                sym,
                utf8: &utf8,
                ctrl,
                shift: st.mod_name_is_active(xkb::MOD_NAME_SHIFT, xkb::STATE_MODS_EFFECTIVE),
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
            keys::Effect::Moved => {
                if self.sel != previous_selection {
                    self.sync_theme_preview();
                    self.ensure_visible();
                }
            }
            keys::Effect::Copy(_) => self.input_command(1),
            keys::Effect::Cut(_) => self.input_command(0),
            keys::Effect::Paste => self.input_command(2),
            keys::Effect::DeleteClip => {
                let position = self.sel;
                if let Some(action) = selected {
                    if self.clip.remove(&action) && self.cfg.clipboard_persist {
                        self.clip.save();
                    }
                }
                self.refresh();
                self.sel = position.min(self.items.len().saturating_sub(1));
                self.ensure_visible();
            }
        }
        self.redraw(qh);
        repeat
    }

    fn scrollbar_geometry(&self) -> Option<scroll::Geometry> {
        self.layout().scroll_geometry(
            &self.items,
            self.view_query(),
            self.cfg.width,
            self.vp(),
            self.top,
            self.scroll_remainder,
        )
    }

    fn scroll_to_offset(&mut self, offset: f32) {
        let before = (self.top, self.scroll_remainder);
        let columns = self.columns();
        let Some(g) = self.scrollbar_geometry() else {
            self.top = 0;
            self.scroll_remainder = 0.0;
            return;
        };
        let offset = offset.clamp(0.0, g.content - g.viewport);
        if self.view_query().starts_with(':') {
            self.top = (offset / self.layout().cell) as usize * columns;
            self.scroll_remainder = offset - (self.top / columns) as f32 * self.layout().cell;
        } else {
            let mut lo = 0;
            let mut hi = self.items.len().saturating_sub(1);
            while lo < hi {
                let mid = (lo + hi).div_ceil(2);
                if self
                    .layout()
                    .scroll_offset(&self.items, mid, self.view_query().starts_with(';'))
                    <= offset
                {
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            self.top = lo;
            self.scroll_remainder = offset
                - self
                    .layout()
                    .scroll_offset(&self.items, lo, self.view_query().starts_with(';'));
        }
        if (self.top, self.scroll_remainder) != before {
            self.scrollbar.changed();
        }
    }

    fn scrollbar_motion(&mut self, qh: &QueueHandle<App>) {
        let old_hover = (self.scrollbar.hovered, self.scrollbar.thumb_hovered);
        let right = if self.view_query().starts_with(';') {
            self.cfg.width as f32 * 0.5
        } else {
            self.cfg.width as f32
        };
        let geometry = self.scrollbar_geometry();
        let point = self
            .pointer
            .map(|(x, y)| (x as f32, y as f32 - ui::HEAD - ui::LIST_PAD));
        let on_bar = point
            .is_some_and(|(x, y)| x >= right - 16.0 && x < right && y >= 0.0 && y < self.vp())
            && geometry.is_some();
        self.scrollbar.hovered = on_bar;
        self.scrollbar.thumb_hovered =
            self.scrollbar.hovered && point.zip(geometry).is_some_and(|((_, y), g)| g.on_thumb(y));
        if old_hover.0 || self.scrollbar.hovered {
            self.scrollbar.changed();
        }
        if let (Some(drag), Some((_, y)), Some(g)) = (self.scrollbar.drag, point, geometry) {
            self.scroll_to_offset(g.offset_at(y, Some(drag)));
            self.redraw(qh);
        } else if old_hover != (self.scrollbar.hovered, self.scrollbar.thumb_hovered) {
            self.redraw(qh);
        }
    }

    fn row_at(&self, x: f64, y: f64) -> Option<usize> {
        if self.view_query().starts_with(';') && x >= self.cfg.width as f64 * 0.5 {
            return None;
        }
        // The clipboard list starts 3px higher than the other lists in the release.
        let origin = if self.view_query().starts_with(';') {
            ui::HEAD + ui::CLIP_TOP
        } else {
            ui::HEAD + ui::LIST_PAD
        };
        if y < origin as f64 || y >= (origin + self.vp()) as f64 {
            return None;
        }
        if self.view_query().starts_with(':') {
            // Clicks below the visible grid (the padding under the viewport)
            // hit nothing; the partially visible row stays clickable.
            if y as f32 > ui::HEAD + ui::LIST_PAD + self.vp() {
                return None;
            }
            return self.layout().emoji_hit(
                self.cfg.width,
                ((self.vp() + self.scroll_remainder) / self.layout().cell).ceil() as usize,
                self.top,
                self.items.len(),
                x as f32,
                y as f32 + self.scroll_remainder,
            );
        }
        self.layout().item_at(
            &self.items,
            self.top,
            self.vp() + self.scroll_remainder,
            y as f32 - origin + self.scroll_remainder,
        )
    }
}

impl App {
    fn init_clip(&mut self, qh: &QueueHandle<App>) {
        // The collector runs only when the Clipboard module is configured,
        // like zlaunch's init_clipboard_if_enabled.
        let in_combined = self
            .cfg
            .combined_modules()
            .contains(&config::ConfigModule::Clipboard);
        let in_modes = self.cfg.default_modes.iter().flatten().any(|m| {
            modes::Mode::parse(m) == Some(modes::Mode::Module(config::ConfigModule::Clipboard))
        });
        if !in_combined && !in_modes {
            return;
        }
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
                    if r.files {
                        self.clip.push_uri_list(&t);
                    } else {
                        self.clip
                            .push(&if r.html { clip::strip_html(&t) } else { t });
                    }
                }
                if self.cfg.clipboard_persist {
                    self.clip.save();
                }
                if self.shown && self.view_query().starts_with(';') {
                    self.refresh();
                    self.redraw(qh);
                }
            } else {
                i += 1;
            }
        }
    }
}

fn copy_input_text(text: &str) -> bool {
    let Ok(mut child) = std::process::Command::new("timeout")
        .args(["1", "wl-copy", "--type", "text/plain;charset=utf-8"])
        .stdin(std::process::Stdio::piped())
        .spawn()
    else {
        return false;
    };
    let written = child
        .stdin
        .take()
        .is_some_and(|mut input| input.write_all(text.as_bytes()).is_ok());
    let success = child.wait().is_ok_and(|status| status.success());
    written && success
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
delegate_noop!(App: ignore WlSubcompositor);
delegate_noop!(App: ignore WlSubsurface);
delegate_noop!(App: ignore WpViewporter);
delegate_noop!(App: ignore WpViewport);
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
            zwlr_data_control_device_v1::Event::PrimarySelection { id: Some(offer) } => {
                a.offers.remove(&offer.id());
                offer.destroy();
            }
            zwlr_data_control_device_v1::Event::Selection { id } => {
                let Some(offer) = id else { return };
                let mimes = a.offers.get(&offer.id()).cloned().unwrap_or_default();
                if !a.cfg.clipboard_history || a.reads.len() >= 8 {
                    a.offers.remove(&offer.id());
                    offer.destroy();
                    return;
                }
                let image = clip::pick_image(&mimes, a.cfg.store_passwords).is_some();
                if let Some(m) = clip::pick_image(&mimes, a.cfg.store_passwords)
                    .or_else(|| clip::pick_mime(&mimes, a.cfg.store_passwords))
                {
                    let html = m == "text/html";
                    let files = m == "text/uri-list";
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
                            files,
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
        surface: &WlSurface,
        ev: wayland_client::protocol::wl_surface::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if a.surface.as_ref() != Some(surface)
            && a.backdrop.as_ref().is_none_or(|b| &b.surface != surface)
        {
            return;
        }
        if let wayland_client::protocol::wl_surface::Event::Enter { output } = ev {
            if let Some(o) = a.outputs.iter().find(|o| o.obj == output) {
                a.active_output = Some(output);
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
        qh: &QueueHandle<Self>,
    ) {
        use wayland_client::protocol::wl_output::Event;
        let Some(e) = a.outputs.iter_mut().find(|e| e.obj == *o) else {
            return;
        };
        match ev {
            Event::Scale { factor } => {
                e.scale = factor.max(1) as u32;
                if a.active_output.as_ref() == Some(o) && a.scale != e.scale {
                    a.scale = e.scale;
                    a.redraw(qh);
                }
            }
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
                "wl_subcompositor" => a.subcompositor = Some(r.bind(name, 1, qh, ())),
                "wp_viewporter" => a.viewporter = Some(r.bind(name, 1, qh, ())),
                "wl_shm" => a.shm = Some(r.bind(name, 1, qh, ())),
                "zwlr_layer_shell_v1" => a.shell = Some(r.bind(name, 1, qh, ())),
                "zwlr_data_control_manager_v1" => {
                    a.dc_manager = Some(r.bind(name, 2.min(version), qh, ()))
                }
                "wl_seat" => a.seat = Some(r.bind(name, 5.min(version), qh, ())),
                "zwp_text_input_manager_v3" => {
                    a.text_manager = Some(r.bind(name, 1, qh, ()));
                    a.setup_text_input(qh);
                }
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

delegate_noop!(App: ignore ZwpTextInputManagerV3);

impl Dispatch<ZwpTextInputV3, ()> for App {
    fn event(
        a: &mut Self,
        input: &ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        use zwp_text_input_v3::Event;
        match event {
            Event::Enter { surface } => {
                if a.shown
                    && (a.surface.as_ref() == Some(&surface)
                        || a.backdrop.as_ref().is_some_and(|b| b.surface == surface))
                {
                    a.ime.clear();
                    a.text_focus = Some(surface);
                    a.text_snapshot = None;
                    a.text_waiting = false;
                    input.enable();
                    a.update_text_input(false);
                }
            }
            Event::Leave { surface } => {
                if a.text_focus.as_ref() == Some(&surface) {
                    a.text_focus = None;
                    if a.ime.clear() {
                        a.refresh();
                    }
                    a.text_snapshot = None;
                    a.redraw(qh);
                }
            }
            Event::PreeditString {
                text,
                cursor_begin,
                cursor_end,
            } if a.text_focus.is_some() => {
                a.ime.pending_preedit = Some(ime::Preedit {
                    text: text.unwrap_or_default(),
                    begin: cursor_begin,
                    end: cursor_end,
                });
            }
            Event::CommitString { text } if a.text_focus.is_some() => a.ime.commit = text,
            Event::DeleteSurroundingText {
                before_length,
                after_length,
            } if a.text_focus.is_some() => a.ime.delete = (before_length, after_length),
            Event::Done { serial } if a.text_focus.is_some() && a.shown => {
                let changed = a.ime.apply(&mut a.query, &mut a.edit);
                a.text_waiting = serial != a.text_commits;
                if changed {
                    a.refresh();
                }
                a.pause_caret();
                a.update_text_input(true);
                a.redraw(qh);
            }
            _ => {}
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
        a.setup_text_input(qh);
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
                    let locale = std::env::var_os("LC_ALL")
                        .filter(|s| !s.is_empty())
                        .or_else(|| std::env::var_os("LC_CTYPE").filter(|s| !s.is_empty()))
                        .or_else(|| std::env::var_os("LANG").filter(|s| !s.is_empty()))
                        .unwrap_or_else(|| "C.UTF-8".into());
                    a.compose = xkb::compose::Table::new_from_locale(
                        &a.ctx,
                        &locale,
                        xkb::compose::COMPILE_NO_FLAGS,
                    )
                    .ok()
                    .map(|t| xkb::compose::State::new(&t, xkb::compose::STATE_NO_FLAGS));
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
                if rate <= 0 {
                    a.repeating = None;
                }
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
            Event::Leave { surface, .. } => {
                // Losing keyboard focus hides the panel, like zlaunch.
                a.repeating = None;
                let current = a
                    .backdrop
                    .as_ref()
                    .map(|b| &b.surface)
                    .or(a.surface.as_ref());
                if a.shown && current == Some(&surface) {
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
            Event::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                a.pointer_on_backdrop = a.backdrop.as_ref().is_some_and(|b| b.surface == surface);
                a.pointer = a
                    .surface
                    .as_ref()
                    .filter(|s| **s == surface)
                    .map(|_| (surface_x, surface_y));

                a.scrollbar_motion(qh);
                if a.pointer.is_some() && a.input_drag {
                    let at = ui::input_at_mode(
                        &a.font,
                        &a.query,
                        a.edit.caret,
                        a.cfg.width,
                        surface_x,
                        a.edit.literal_prefix,
                    );
                    a.edit.move_to(&a.query, at, true);
                    a.redraw(qh);
                }
            }
            Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                a.pointer = (!a.pointer_on_backdrop).then_some((surface_x, surface_y));

                if let Some(menu) = &mut a.input_menu {
                    menu.selected = menu.at(surface_x, surface_y);
                    a.redraw(qh);
                    return;
                }

                a.scrollbar_motion(qh);
                if a.pointer.is_some() && a.input_drag {
                    let at = ui::input_at_mode(
                        &a.font,
                        &a.query,
                        a.edit.caret,
                        a.cfg.width,
                        surface_x,
                        a.edit.literal_prefix,
                    );
                    a.edit.move_to(&a.query, at, true);
                    a.redraw(qh);
                }
            }
            Event::Leave { .. } => {
                a.pointer = None;
                a.input_drag = false;
                a.pointer_on_backdrop = false;
                if a.scrollbar.hovered {
                    a.scrollbar.changed();
                }
                a.scrollbar.hovered = false;
                a.scrollbar.thumb_hovered = false;
                a.scrollbar.drag = None;
            }
            Event::Button {
                button: 272,
                state: WEnum::Value(ButtonState::Pressed),
                ..
            } => {
                a.last_click = None;
                if a.ime.clear() {
                    a.refresh();
                }
                if let Some(menu) = &mut a.input_menu {
                    if let Some((x, y)) = a.pointer {
                        if menu.contains(x, y) {
                            menu.pressed = menu.at(x, y);
                            return;
                        }
                    }
                    a.input_menu = None;
                    a.redraw(qh);
                    return;
                }
                if a.shown && a.pointer_on_backdrop {
                    a.hide();
                    return;
                }

                if a.shown && a.scrollbar.hovered {
                    if let (Some((_, y)), Some(g)) = (a.pointer, a.scrollbar_geometry()) {
                        let y = y as f32 - ui::HEAD - ui::LIST_PAD;
                        if g.on_thumb(y) {
                            a.scrollbar.drag = Some(y - g.start);
                        } else {
                            a.scroll_to_offset(g.offset_at(y, None));
                        }
                        a.scrollbar.changed();
                        a.redraw(qh);
                        return;
                    }
                }

                // The back arrow returns to an unfiltered combined view.
                if a.shown
                    && a.origin.is_some()
                    && a.pointer.is_some_and(|(x, y)| ui::back_button_at(x, y))
                {
                    a.origin = None;
                    a.query.clear();
                    a.last_click = None;
                    a.refresh();
                    a.redraw(qh);
                    return;
                }
                if a.shown
                    && a.pointer.is_some_and(|(x, y)| {
                        ui::clear_button_at_mode(a.cfg.width, &a.query, x, y, a.edit.literal_prefix)
                    })
                {
                    a.edit.sync(&a.query);
                    a.edit.select_all(&a.query);
                    a.edit.replace(&mut a.query, "");
                    a.last_click = None;
                    a.refresh();
                    a.redraw(qh);
                    return;
                }
                if let Some((x, y)) = a.pointer.filter(|(_, y)| *y >= 0.0 && *y < ui::HEAD as f64) {
                    let _ = y;
                    a.edit.sync(&a.query);
                    let at = ui::input_at_mode(
                        &a.font,
                        &a.query,
                        a.edit.caret,
                        a.cfg.width,
                        x,
                        a.edit.literal_prefix,
                    );
                    let now = Instant::now();
                    let count = a
                        .input_click
                        .filter(|(t, _, px, py)| {
                            now.duration_since(*t) < Duration::from_millis(400)
                                && (x - px).abs() < 4.0
                                && (y - py).abs() < 4.0
                        })
                        .map_or(1, |(_, n, _, _)| n.saturating_add(1));
                    a.input_click = Some((now, count, x, y));
                    if count >= 3 {
                        a.edit.select_all(&a.query);
                    } else if count == 2 {
                        a.edit.select_word(&a.query, at);
                    } else {
                        let shift = a.xkb.as_ref().is_some_and(|s| {
                            s.mod_name_is_active(xkb::MOD_NAME_SHIFT, xkb::STATE_MODS_EFFECTIVE)
                        });
                        a.edit.move_to(&a.query, at, shift);
                    }
                    a.input_drag = true;
                    a.pause_caret();
                    a.redraw(qh);
                    return;
                }
                let row = a.pointer.and_then(|(x, y)| a.row_at(x, y));
                a.last_click = row.map(|i| (i, Instant::now()));
            }
            Event::Button {
                button: 272,
                state: WEnum::Value(ButtonState::Released),
                ..
            } => {
                if let Some(menu) = &a.input_menu {
                    let released = a.pointer.and_then(|(x, y)| menu.at(x, y));
                    let command = menu
                        .pressed
                        .filter(|&i| Some(i) == released && menu.enabled[i]);
                    if let Some(command) = command {
                        a.input_menu = None;
                        a.input_command(command);
                        a.redraw(qh);
                    }
                    return;
                }
                a.input_drag = false;
                a.edit.end_drag();
                if a.scrollbar.drag.take().is_some() {
                    a.scrollbar.changed();
                    a.redraw(qh);
                    return;
                }
                let pressed = a.last_click.take().map(|(i, _)| i);
                let released = a.pointer.and_then(|(x, y)| a.row_at(x, y));
                if let Some(i) = pressed.filter(|_| a.shown && pressed == released) {
                    // zlaunch: a vlist row click confirms the row's first item,
                    // so any emoji grid cell copies that row's first emoji.
                    a.sel = if a.view_query().starts_with(':') {
                        i - (i % a.columns())
                    } else {
                        i
                    };
                    a.activate(qh);
                }
            }
            Event::Button {
                button: 273,
                state: WEnum::Value(ButtonState::Pressed),
                ..
            } => {
                if let Some((x, y)) = a.pointer.filter(|(_, y)| *y >= 0.0 && *y < ui::HEAD as f64) {
                    a.edit.sync(&a.query);
                    let at = ui::input_at_mode(
                        &a.font,
                        &a.query,
                        a.edit.caret,
                        a.cfg.width,
                        x,
                        a.edit.literal_prefix,
                    );
                    if !a.edit.range().is_some_and(|r| r.contains(&at)) {
                        a.edit.move_to(&a.query, at, false);
                    }
                    let paste = std::process::Command::new("timeout")
                        .args(["1", "wl-paste", "--list-types"])
                        .output()
                        .is_ok_and(|o| o.status.success() && !o.stdout.is_empty());
                    a.input_menu = Some(input_menu::Menu::new(
                        x,
                        y,
                        a.cfg.width,
                        a.cur_h,
                        a.edit.range().is_some(),
                        paste,
                    ));
                    a.redraw(qh);
                }
            }
            Event::AxisDiscrete {
                axis: WEnum::Value(Axis::VerticalScroll),
                ..
            } => {
                a.wheel_discrete = true;
            }
            Event::Axis {
                axis: WEnum::Value(Axis::VerticalScroll),
                value,
                ..
            } => {
                // Measured against the 0.5.1 release: one discrete wheel step of 60 scrolls
                // 78px there. Pixel-valued (touchpad) deltas stay unscaled.
                let value = if std::mem::take(&mut a.wheel_discrete) {
                    value * 1.3
                } else {
                    value
                };
                if a.pointer_on_backdrop || a.input_menu.is_some() || value == 0.0 {
                    return;
                }

                let right = if a.view_query().starts_with(';') {
                    a.cfg.width as f64 * 0.5
                } else {
                    a.cfg.width as f64
                };
                if a.pointer_on_backdrop
                    || a.pointer.is_none_or(|(x, y)| {
                        x < 0.0
                            || x >= right
                            || y < (ui::HEAD + ui::LIST_PAD) as f64
                            || y >= (ui::HEAD + ui::LIST_PAD + a.vp()) as f64
                    })
                {
                    return;
                }

                if let Some(g) = a.scrollbar_geometry() {
                    let offset = if a.view_query().starts_with(':') {
                        (a.top / a.columns()) as f32 * a.layout().cell
                    } else {
                        a.layout()
                            .scroll_offset(&a.items, a.top, a.view_query().starts_with(';'))
                    } + a.scroll_remainder;
                    a.scroll_to_offset((offset + value as f32).clamp(0.0, g.content - g.viewport));
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
        if a.layer.as_ref() != Some(ls) {
            return;
        }
        match ev {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                ls.ack_configure(serial);
                if !a.configure_backdrop(width, height, qh) {
                    eprintln!("zl: cannot allocate the outside-click surface");
                    a.hide();
                    return;
                }
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
    cfg.font_file(false)
        .into_iter()
        .chain(["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into()])
        .find_map(|p| std::fs::read(p).ok().and_then(Font::try_from_vec))
        .unwrap_or_else(|| {
            eprintln!("zl: no usable font found");
            std::process::exit(1)
        })
}

fn load_bold_font(cfg: &Config) -> Option<Font<'static>> {
    cfg.font_file(true)
        .into_iter()
        .chain(["/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf".into()])
        .find_map(|p| std::fs::read(p).ok().and_then(Font::try_from_vec))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let wire = match cli::parse(&args) {
        cli::Cmd::Daemon => None,
        cli::Cmd::Print(text, code) => {
            if code == 0 {
                let _ = write!(std::io::stdout(), "{text}");
            } else {
                eprint!("{text}");
            }
            std::process::exit(code);
        }
        cli::Cmd::ThemeCurrent => Some("theme-current".into()),
        cli::Cmd::ThemeList => Some("theme-list".into()),
        cli::Cmd::Show(modes) => Some(format!("show {}", cli::show_modes(&modes))),
        cli::Cmd::Toggle(modes) => Some(format!("toggle {}", cli::show_modes(&modes))),
        cli::Cmd::Hide => Some("hide".into()),
        cli::Cmd::Quit => Some("quit".into()),
        cli::Cmd::Reload => Some("reload".into()),
        cli::Cmd::ThemeSet(name) => Some(format!("theme-set {name}")),
    };
    if let Some(wire) = wire {
        match UnixStream::connect(sock()) {
            Ok(mut s) => {
                let result = (|| -> std::io::Result<String> {
                    s.set_read_timeout(Some(Duration::from_secs(2)))?;
                    s.set_write_timeout(Some(Duration::from_secs(2)))?;
                    s.write_all(wire.as_bytes())?;
                    s.shutdown(std::net::Shutdown::Write)?;
                    control_io::read_message(&mut s, 4 * 1024 * 1024, Duration::from_secs(2))
                })();
                match result {
                    Ok(reply) if reply.starts_with("ok\n") => {
                        let payload = &reply[3..];
                        let mut out = std::io::stdout().lock();
                        if wire == "theme-current" {
                            let _ = writeln!(out, "Current theme: {}", payload.trim_end());
                        } else if wire == "theme-list" {
                            let _ = write!(out, "Available themes:\n{payload}");
                        } else if let Some(name) = wire.strip_prefix("theme-set ") {
                            let _ = writeln!(out, "Theme set to '{name}'");
                        } else if wire == "reload" {
                            let _ = writeln!(out, "Daemon is reloading...");
                        }
                    }
                    Ok(reply) => {
                        eprintln!(
                            "Error: {}",
                            reply
                                .strip_prefix("err ")
                                .unwrap_or("Invalid daemon response")
                                .trim_end()
                        );
                        std::process::exit(1);
                    }
                    Err(error) => {
                        eprintln!("Error: daemon request failed: {error}");
                        std::process::exit(1);
                    }
                }
            }
            Err(_) => {
                eprintln!("Error: zl daemon is not running. Start it first by running: zl");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Err(error) = run_daemon() {
        eprintln!("zl: daemon stopped: {error}");
        std::process::exit(1);
    }
}

fn run_daemon() -> Result<(), Box<dyn std::error::Error>> {
    launch::capture_environment();
    let cfg = Config::load();
    if UnixStream::connect(sock()).is_ok() {
        eprintln!("zl: already running");
        return Ok(());
    }
    let _ = std::fs::remove_file(sock());
    if let Err(error) = hyprland_blur::apply(cfg.hyprland_auto_blur) {
        eprintln!("zl: failed to apply Hyprland blur rules: {error}");
    }
    let listener = UnixListener::bind(sock())?;
    let _socket_path = socket_path::SocketPath::new(sock())?;
    listener.set_nonblocking(true)?;
    let conn = Connection::connect_to_env()?;
    let mut queue = conn.new_event_queue::<App>();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut app = App {
        theme: cfg.theme(),
        saved_theme_name: if config::load_theme(&cfg.theme).is_some() {
            cfg.theme.clone()
        } else {
            "default".into()
        },
        icons: icons::Cache::new(&cfg.icon_theme, &cfg.emoji_font),
        font: load_font(&cfg),
        font_size: cfg.startup_font_size(),
        mono_font_family: cfg.startup_fonts().mono_font_family,
        bold_font: load_bold_font(&cfg),
        apps: app_cache::load(&cfg),
        launched: vec![],
        windows: vec![],
        items: vec![],
        query: String::new(),
        edit: input::Edit::default(),
        ime: ime::State::default(),
        text_manager: None,
        text_input: None,
        text_focus: None,
        text_commits: 0,
        text_waiting: false,
        text_snapshot: None,
        caret_visible: true,
        caret_next: Instant::now(),
        sel: 0,
        top: 0,
        scroll_remainder: 0.0,
        scrollbar: scroll::State::default(),
        shown: false,
        configured: false,
        cur_h: 0,
        scale: 1,
        outputs: vec![],
        active_output: None,
        repeat_rate: 25,
        repeat_delay: 400,
        repeating: None,
        pointer: None,
        pointer_on_backdrop: false,
        last_click: None,
        wheel_discrete: false,
        origin: None,
        input_drag: false,
        input_menu: None,
        input_click: None,
        settings_error: None,
        hist: hist::Hist::load(),
        previewing: false,
        theme_catalog: None,
        theme_palettes: std::collections::HashMap::new(),
        modes: modes::State::new(cfg.default_modes.as_deref()),
        cfg,
        compositor: None,
        subcompositor: None,
        viewporter: None,
        backdrop: None,
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
        compose: None,
        clip: clip::History::new(0),
        dc_manager: None,
        dc_device: None,
        offers: Default::default(),
        reads: vec![],
        own_source: None,
    };
    app.clip = if app.cfg.clipboard_persist {
        clip::History::load(app.cfg.clipboard_limit())
    } else {
        clip::History::new(app.cfg.clipboard_limit())
    };
    queue.roundtrip(&mut app)?;
    queue.roundtrip(&mut app)?;
    app.init_clip(&qh);
    if app.shell.is_none() {
        return Err("the compositor has no wlr-layer-shell support".into());
    }
    let mut app_watch = app_watch::AppWatch::new()
        .map_err(|error| eprintln!("zl: cannot start application watcher: {error}"))
        .ok();
    loop {
        launch::reap(&mut app.launched);
        queue.dispatch_pending(&mut app)?;
        queue.flush()?;
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
        if let Some(watch) = &app_watch {
            fds.push(libc::pollfd {
                fd: watch.fd(),
                events: libc::POLLIN,
                revents: 0,
            });
        }
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
        if app.shown {
            let wait = app
                .caret_next
                .saturating_duration_since(Instant::now())
                .as_millis() as i32
                + 1;
            timeout = if timeout < 0 { wait } else { timeout.min(wait) };
        }
        if let Some(wait) = app_watch.as_ref().and_then(|watch| watch.wait_ms()) {
            timeout = if timeout < 0 { wait } else { timeout.min(wait) };
        }
        if app.shown {
            if let Some(wait) = app.scrollbar.next_frame() {
                let wait = wait.as_millis() as i32 + 1;
                timeout = if timeout < 0 { wait } else { timeout.min(wait) };
            }
        }
        if !app.reads.is_empty() || !app.launched.is_empty() {
            timeout = if timeout < 0 { 500 } else { timeout.min(500) };
        }
        unsafe {
            libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout);
        }
        if app_watch.as_mut().is_some_and(|watch| watch.changed()) {
            app.apps = app_cache::load(&app.cfg);
            if app.shown {
                app.refresh();
                app.redraw(&qh);
            }
        }
        app.finish_reads(&qh);
        if app.shown && Instant::now() >= app.caret_next {
            app.caret_visible = !app.caret_visible;
            app.caret_next = Instant::now() + Duration::from_millis(500);
            app.redraw(&qh);
        }
        if app.shown
            && !app.scrollbar.hovered
            && app.scrollbar.drag.is_none()
            && app
                .scrollbar
                .last_scroll
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(2))
        {
            app.redraw(&qh);
            app.scrollbar.finish_frame();
        }
        if fds[0].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
            if let Err(error) = guard.read() {
                if !matches!(&error, wayland_client::backend::WaylandError::Io(e) if e.kind() == std::io::ErrorKind::WouldBlock)
                {
                    return Err(error.into());
                }
            }
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
                s.set_write_timeout(Some(Duration::from_millis(200))).ok();
                let b =
                    match control_io::read_message(&mut s, 64 * 1024, Duration::from_millis(200)) {
                        Ok(message) => message,
                        Err(error) => {
                            let _ = writeln!(s, "err Invalid control request: {error}");
                            continue;
                        }
                    };
                let (word, arg) = b.split_once(' ').unwrap_or((b.as_str(), ""));
                match word {
                    "show" => {
                        if !app.shown {
                            app.set_modes(arg);
                        }
                        app.show(&qh);
                        let _ = s.write_all(b"ok\n");
                    }
                    "hide" => {
                        app.hide();
                        let _ = s.write_all(b"ok\n");
                    }
                    "toggle" => {
                        if app.shown {
                            app.hide()
                        } else {
                            app.set_modes(arg);
                            app.show(&qh);
                        }
                        let _ = s.write_all(b"ok\n");
                    }
                    "reload" => {
                        let _ = s.write_all(b"ok\n");
                        let _ = std::fs::remove_file(sock());
                        if let Ok(exe) = std::env::current_exe() {
                            let err = std::process::Command::new(exe).exec();
                            eprintln!("zl: reload failed: {err}");
                        }
                        return Ok(());
                    }
                    "theme-set" => {
                        let ok = config::load_theme(arg).is_some() && {
                            let ok = config::set_theme(&mut app.cfg, arg);
                            app.saved_theme_name = arg.into();
                            app.theme = app.cfg.theme();
                            app.redraw(&qh);
                            ok
                        };
                        let reply = if ok {
                            "ok\n".into()
                        } else {
                            format!("err Theme '{arg}' not found\n")
                        };
                        let _ = s.write_all(reply.as_bytes());
                    }
                    "theme-current" => {
                        let _ = writeln!(s, "ok\n{}", app.saved_theme_name);
                    }
                    "theme-list" => {
                        let _ = s.write_all(b"ok\n");
                        for (name, source) in config::available_themes() {
                            let kind = if source == "bundled" {
                                "bundled"
                            } else {
                                "user"
                            };
                            if writeln!(s, "  {name} ({kind})").is_err() {
                                break;
                            }
                        }
                    }
                    "quit" => {
                        let _ = s.write_all(b"ok\n");
                        let _ = std::fs::remove_file(sock());
                        return Ok(());
                    }
                    _ => {
                        let _ = s.write_all(b"err Unknown command\n");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod backdrop_tests {
    use super::*;

    #[test]
    fn explicit_window_size_is_clamped_to_the_panel() {
        let mut cfg = Config {
            window_size: Some((900.5, 700.0)),
            ..Config::default()
        };
        assert_eq!(explicit_window_size(&cfg, 400), Some((901, 700)));
        cfg.window_size = Some((1.0, -1.0));
        assert_eq!(explicit_window_size(&cfg, 400), Some((cfg.width, 400)));
        cfg.enable_backdrop = false;
        assert_eq!(explicit_window_size(&cfg, 400), None);
        cfg.enable_backdrop = true;
        cfg.window_size = Some((f32::NAN, f32::INFINITY));
        assert_eq!(explicit_window_size(&cfg, 400), None);
    }

    #[test]
    fn viewport_backdrop_uses_one_pixel_at_any_output_size() {
        for (w, h) in [(1280, 800), (1920, 1080), (3840, 2160), (7680, 4320)] {
            assert_eq!(backdrop_buffer_size(w, h, true), Some((1, 1, 4)));
            assert_eq!(
                backdrop_buffer_size(w, h, false),
                Some((w, h, w as usize * h as usize * 4))
            );
        }
    }

    #[test]
    fn invalid_backdrop_sizes_are_rejected_before_allocation() {
        for (w, h) in [(0, 100), (100, 0), (u32::MAX, 1), (1, u32::MAX)] {
            assert_eq!(backdrop_buffer_size(w, h, false), None);
            assert_eq!(backdrop_buffer_size(w, h, true), None);
        }
        assert_eq!(backdrop_buffer_size(16384, 8192, false), None);
        assert_eq!(backdrop_buffer_size(16384, 8192, true), Some((1, 1, 4)));
    }

    #[test]
    fn panel_is_centered_in_logical_output_coordinates() {
        assert_eq!(panel_position(1920, 1080, 640, 470), (640, 305));
        assert_eq!(panel_position(960, 540, 640, 470), (160, 35));
        assert_eq!(panel_position(1280, 800, 600, 400), (340, 200));
        assert_eq!(panel_position(400, 300, 600, 400), (-100, -50));
    }
}

#[cfg(test)]
mod picker_tests {
    use super::*;
    fn app(cfg: Config) -> App {
        App {
            theme: cfg.theme(),
            saved_theme_name: if config::load_theme(&cfg.theme).is_some() {
                cfg.theme.clone()
            } else {
                "default".into()
            },
            icons: icons::Cache::new(&cfg.icon_theme, &cfg.emoji_font),
            font: load_font(&cfg),
            font_size: cfg.startup_font_size(),
            mono_font_family: cfg.startup_fonts().mono_font_family,
            bold_font: load_bold_font(&cfg),
            apps: items::scan_apps(&cfg),
            launched: vec![],
            windows: vec![],
            items: vec![],
            query: String::new(),
            edit: input::Edit::default(),
            ime: ime::State::default(),
            text_manager: None,
            text_input: None,
            text_focus: None,
            text_commits: 0,
            text_waiting: false,
            text_snapshot: None,
            caret_visible: true,
            caret_next: Instant::now(),
            sel: 0,
            top: 0,
            scroll_remainder: 0.0,
            scrollbar: scroll::State::default(),
            shown: false,
            configured: false,
            cur_h: 0,
            scale: 1,
            outputs: vec![],
            active_output: None,
            repeat_rate: 25,
            repeat_delay: 400,
            repeating: None,
            pointer: None,
            last_click: None,
            wheel_discrete: false,
            pointer_on_backdrop: false,
            origin: None,
            input_drag: false,
            input_menu: None,
            input_click: None,
            settings_error: None,
            previewing: false,
            theme_catalog: None,
            theme_palettes: std::collections::HashMap::new(),
            hist: hist::Hist::default(),
            modes: modes::State::new(cfg.default_modes.as_deref()),
            cfg,
            compositor: None,
            subcompositor: None,
            viewporter: None,
            backdrop: None,
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
            compose: None,
            clip: clip::History::new(0),
            dc_manager: None,
            dc_device: None,
            offers: Default::default(),
            reads: vec![],
            own_source: None,
        }
    }
    fn scroll_fixture(query: &str) -> App {
        let mut a = app(Config::default());
        a.query = query.into();
        a.items = (0..20)
            .map(|i| Item {
                title: i.to_string(),
                sub: String::new(),
                tag: "Theme",
                icon: None,
                action: Action::Theme("default".into()),
                best: false,
            })
            .collect();
        a
    }

    #[test]
    fn pixel_scroll_keeps_selection_and_remainder_and_clamps_both_ends() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let mut a = scroll_fixture("#");
        a.scroll_to_offset(123.5);
        assert_eq!((a.top, a.scroll_remainder, a.sel), (2, 23.5, 0));
        a.scroll_to_offset(f32::MAX);
        assert_eq!((a.top, a.scroll_remainder, a.sel), (13, 24.0, 0));
        a.scroll_to_offset(-1.0);
        assert_eq!((a.top, a.scroll_remainder), (0, 0.0));
        a.items.clear();
        a.scroll_to_offset(100.0);
        assert_eq!((a.top, a.scroll_remainder), (0, 0.0));
    }

    #[test]
    fn navigation_keeps_visible_rows_in_place_then_reveals_only_the_clipped_part() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let mut a = scroll_fixture("#");
        a.sel = 1;
        a.ensure_visible();
        assert_eq!((a.top, a.scroll_remainder), (0, 0.0));
        a.sel = 6;
        a.ensure_visible();
        assert_eq!((a.top, a.scroll_remainder), (0, 24.0));
        a.sel = 0;
        a.ensure_visible();
        assert_eq!((a.top, a.scroll_remainder), (0, 0.0));
        a.query = ":".into();
        a.items = (0..100).map(|_| a.items[0].clone()).collect();
        a.sel = a.columns();
        a.ensure_visible();
        assert_eq!((a.top, a.scroll_remainder), (0, 0.0));
        a.sel = a.columns() * 5;
        a.ensure_visible();
        assert_eq!((a.top, a.scroll_remainder), (0, 58.0));
    }

    #[test]
    fn pixel_scrolled_rows_use_the_visible_part_for_pointer_hits() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let mut a = scroll_fixture("#");
        a.scroll_to_offset(123.5);
        assert_eq!(a.row_at(100.0, 66.0), Some(2));
        assert_eq!(a.row_at(100.0, 93.0), Some(3));
        assert_eq!(a.row_at(100.0, 65.9), None);
        assert_eq!(a.row_at(100.0, 392.0), None);
        a.query = ";".into();
        assert_eq!(a.row_at(400.0, 100.0), None);
    }

    #[test]
    fn clipboard_theme_pitch_is_shared_by_scroll_and_pointer_hits() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let mut a = scroll_fixture(";");
        a.theme.metrics.item_content_height += 20.0;
        let pitch = ui::CLIP_ROW + 20.0;
        assert_eq!(a.layout().row, pitch);
        assert_eq!(
            a.scrollbar_geometry().unwrap().content,
            a.items.len() as f32 * pitch
        );
        let top = ui::HEAD + ui::CLIP_TOP;
        assert_eq!(a.row_at(100.0, (top + pitch - 0.1) as f64), Some(0));
        assert_eq!(a.row_at(100.0, (top + pitch + 0.1) as f64), Some(1));
        a.scroll_to_offset(pitch + 5.0);
        assert_eq!(a.top, 1);
        assert_eq!(a.scroll_remainder, 5.0);
    }

    #[test]
    fn direct_nonpicker_prefixes_keep_literal_input_and_regular_geometry() {
        for mode in ["apps", "search", "calculator", "actions", "windows"] {
            let mut a = app(Config {
                default_modes: Some(vec![mode.into()]),
                ..Config::default()
            });
            a.apps = vec![items::App {
                name: ":foo".into(),
                exec: "true".into(),
                comment: String::new(),
                icon: String::new(),
                terminal: false,
            }];
            for query in [":foo", ";foo", "#foo"] {
                a.query = query.into();
                a.refresh();
                assert!(a.edit.literal_prefix);
                assert_eq!(a.view_query(), "");
                assert_eq!(a.columns(), 1);
                assert_eq!(a.layout().row, ui::Layout::new(&a.theme.metrics).row);
                assert!(a
                    .items
                    .iter()
                    .all(|item| !matches!(item.action, Action::Clip(..) | Action::Theme(_))));
                if mode == "search" && query == ":foo" {
                    assert!(a.items.iter().any(|item| item.action
                        == Action::WebSearch("https://www.google.com/search?q=%3Afoo".into())));
                    assert_eq!(a.items.len(), 4);
                }
                if mode == "apps" && query == ":foo" {
                    assert_eq!(a.items[0].title, ":foo");
                }
                a.edit.select_all(&a.query);
                assert_eq!(&a.query[a.edit.range().unwrap()], query);
                a.edit.replace(&mut a.query, "");
                assert_eq!(a.query, "");
            }
        }
    }

    #[test]
    fn calculator_and_actions_modes_limit_their_producers() {
        for (mode, query, count) in [("calculator", "1+2", 1), ("actions", "", 5)] {
            let mut a = app(Config {
                default_modes: Some(vec![mode.into()]),
                ..Config::default()
            });
            a.query = query.into();
            a.refresh();
            assert_eq!(a.items.len(), count);
            assert!(a.items.iter().all(|i| if mode == "calculator" {
                matches!(i.action, Action::Calculate(_))
            } else {
                matches!(i.action, Action::System(_))
            }));
        }
    }
    #[test]
    fn active_mode_limits_the_search_module_without_mutating_config() {
        let cfg = Config {
            default_modes: Some(vec!["apps".into(), "clipboard".into()]),
            ..Config::default()
        };
        let mut a = app(cfg);
        a.apps = vec![items::App {
            name: "Fixture app".into(),
            exec: "true".into(),
            comment: String::new(),
            icon: String::new(),
            terminal: false,
        }];
        a.query.clear();
        a.refresh();
        assert_eq!(a.items.len(), 1);
        assert_eq!(a.items[0].title, "Fixture app");
        assert!(matches!(a.items[0].action, Action::App(_)));
        assert!(a.cfg.combined_modules.is_none());
        assert_eq!(a.modes.current().placeholder(), "Search applications...");
        assert!(a.modes.cycle(false));
        a.query = a.modes.current().prefix().into();
        a.clip.max = 10;
        assert!(a.clip.push("fixture"));
        a.refresh();
        assert_eq!(a.query, ";");
        assert_eq!(a.items.len(), 1);
        assert_eq!(a.items[0].title, "fixture");
    }

    #[test]
    fn refresh_keeps_search_order_despite_saved_pins_and_usage() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("zl-rank-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("zl")).unwrap();
        let previous = std::env::var_os("XDG_STATE_HOME");
        std::env::set_var("XDG_STATE_HOME", &dir);
        std::fs::write(dir.join("zl/history"), "100\t1\tApplication:Alphi\n").unwrap();
        std::fs::write(dir.join("zl/pins"), "Application:Alphi\n").unwrap();
        let mut a = app(Config::default());
        a.hist = hist::Hist::load();
        assert!(a.hist.is_pinned("Application:Alphi"));
        assert_eq!(a.hist.stats("Application:Alphi"), Some((100, 1)));
        a.apps = ["Alpha", "Alphi"]
            .into_iter()
            .map(|name| items::App {
                name: name.into(),
                exec: "true".into(),
                comment: String::new(),
                icon: String::new(),
                terminal: false,
            })
            .collect();
        for query in ["", "al"] {
            let expected: Vec<_> = Search {
                cfg: &a.cfg,
                apps: &a.apps,
                windows: &[],
                history: &a.clip,
            }
            .run(query)
            .into_iter()
            .map(|row| (row.title, row.best))
            .collect();
            a.query = query.into();
            a.refresh();
            assert_eq!(
                a.items
                    .iter()
                    .map(|row| (row.title.clone(), row.best))
                    .collect::<Vec<_>>(),
                expected,
                "query {query:?}"
            );
        }
        if let Some(previous) = previous {
            std::env::set_var("XDG_STATE_HOME", previous);
        } else {
            std::env::remove_var("XDG_STATE_HOME");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn theme_catalog_is_a_snapshot_until_the_picker_closes() {
        let _lock = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let old = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        let themes = Config::path().parent().unwrap().join("themes");
        std::fs::create_dir_all(&themes).unwrap();
        std::fs::write(themes.join("custom.toml"), "window_background = '#ff0000'").unwrap();
        let mut a = app(Config::default());
        a.query = "#custom".into();
        a.refresh();
        a.sync_theme_preview();
        assert_eq!(a.theme.bg, [255, 0, 0]);
        std::fs::write(themes.join("custom.toml"), "window_background = '#0000ff'").unwrap();
        std::fs::write(themes.join("new.toml"), "").unwrap();
        a.query = "#".into();
        a.refresh();
        assert!(!a.items.iter().any(|item| item.title == "new"));
        a.query = "#custom".into();
        a.refresh();
        a.sync_theme_preview();
        assert_eq!(a.theme.bg, [255, 0, 0]);
        assert_eq!(a.theme_palettes["custom"].bg, [255, 0, 0]);
        a.hide();
        a.refresh();
        a.sync_theme_preview();
        assert_eq!(a.theme.bg, [0, 0, 255]);
        match old {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    #[test]
    fn opaque_background_stays_opaque_during_cached_theme_preview() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let mut a = app(Config {
            enable_transparency: false,
            ..Config::default()
        });
        a.query = "#nord".into();
        a.refresh();
        a.sync_theme_preview();
        assert_eq!(a.theme.bg, config::theme("nord").bg);
        assert_eq!(a.theme.bg_alpha, 1.0);
        a.query.clear();
        a.refresh();
        a.sync_theme_preview();
        assert_eq!(a.theme.bg_alpha, 1.0);
    }

    #[test]
    fn hiding_discards_input_click_count_before_the_next_panel() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let mut a = app(Config::default());
        a.input_click = Some((Instant::now(), 2, 65.0, 29.0));
        a.hide();
        assert!(a.input_click.is_none());
    }

    #[test]
    fn configured_height_drives_viewport_and_emoji_paging() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let mut a = app(Config {
            launcher_size: Some((720.0, 480.0)),
            width: 720,
            ..Config::default()
        });
        assert_eq!(a.panel_height(), 480);
        assert_eq!(a.vp(), 406.0);
        a.query = ":".into();
        a.refresh();
        assert_eq!((a.vp() / a.layout().cell).floor() as usize, 6);
        a.cfg.launcher_size = Some((720.0, 100.0));
        assert_eq!(a.vp(), 26.0);
        assert_eq!((a.vp() / a.layout().cell).floor().max(1.0) as usize, 1);
    }

    #[test]
    fn preview_never_changes_config_and_leaving_restores_it() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let cfg = Config {
            theme: "nord".into(),
            font: config::FontConfig {
                font_family: Some("DejaVu Sans".into()),
                mono_font_family: Some("DejaVu Sans Mono".into()),
                font_size: Some(17.0),
            },
            ..Config::default()
        };
        let mut a = app(cfg);
        a.query = "#dracula".into();
        a.refresh();
        a.sync_theme_preview();
        assert_eq!(a.cfg.theme, "nord");
        assert_eq!(a.font_size, 17.0);
        assert_eq!(a.mono_font_family.as_deref(), Some("DejaVu Sans Mono"));
        assert_eq!(a.theme, config::theme("dracula"));
        assert!(a.previewing);
        a.query = "#not-a-theme".into();
        a.refresh();
        a.sync_theme_preview();
        assert!(a.items.is_empty());
        assert_eq!(a.theme, config::theme("dracula"));
        a.query.clear();
        a.refresh();
        a.sync_theme_preview();
        assert_eq!(a.theme, config::theme("nord"));
        assert!(!a.previewing);
        a.query = "#catppuccin".into();
        a.refresh();
        a.sync_theme_preview();
        assert_eq!(a.theme, config::theme("catppuccin-latte"));
        a.sel = 1;
        a.sync_theme_preview();
        assert_eq!(a.theme, config::theme("catppuccin-mocha"));
        a.hide();
        assert_eq!(a.theme, config::theme("nord"));
        assert_eq!(a.cfg.theme, "nord");
        assert!(!a.previewing);
    }

    #[test]
    fn manifest_drops_dependencies_nothing_uses() {
        // llm, tokio, futures, anyhow and pulldown-cmark were inherited from the
        // zlaunch port but no rl source file references them.
        let manifest =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
        for removed in ["llm", "tokio", "futures", "anyhow", "pulldown-cmark"] {
            assert!(
                !manifest
                    .lines()
                    .any(|line| line.starts_with(removed) && line.contains('=')),
                "unused dependency {removed} is back in Cargo.toml"
            );
        }
    }

    #[test]
    fn readme_matches_the_implemented_surface() {
        let readme =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md")).unwrap();
        // Calculator, actions and windows modes exist; the README must not deny them.
        assert!(!readme.contains("file search, calculator, currency conversion"));
        assert!(readme.contains("emojis, calculator, clipboard, actions,"));
        assert!(readme.contains("search, themes and windows."));
        // Images share the history limit with text; there is no eight-image cap.
        assert!(!readme.contains("at most eight images"));
        // The single_click key is ignored, not an activation switch.
        assert!(!readme.contains("single-click activation is a config key"));
        // Emoji order is crate order; picked emojis are not promoted.
        assert!(!readme.contains("most recent copies appear first"));
        // The packaging templates and their @BINARY@ placeholder are documented.
        assert!(readme.contains("@BINARY@"));
        // hyprland_auto_blur drives real Hyprland layer rules, not a no-op.
        assert!(!readme.contains("parsed but has no effect"));
    }

    #[test]
    fn notice_names_every_top_level_icon() {
        // Each shipped icon needs a license attribution; brain.svg went unlisted.
        let notice =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/NOTICE")).unwrap();
        let icons =
            std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icons")).unwrap();
        for entry in icons {
            let name = entry.unwrap().file_name().into_string().unwrap();
            if let Some(stem) = name.strip_suffix(".svg") {
                assert!(
                    notice.contains(stem),
                    "NOTICE does not name assets/icons/{name}"
                );
            }
        }
    }
}
