//! zl: a small Wayland launcher. One daemon, a unix socket, a layer-shell panel drawn on the CPU.
mod actions_ui;
mod clip;
mod clipboard_ui;
mod config;
#[allow(dead_code)]
mod dates;
mod details;
#[allow(dead_code)]
mod hist;
mod icons;
mod input;
mod items;
mod keys;
mod scripts;
mod settings;
#[cfg(test)]
mod testutil;
mod ui;

/// A keyboard Leave that is not followed by an Enter within this long hides the panel.
const FOCUS_GRACE: Duration = Duration::from_millis(250);

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

struct ScriptForm {
    script: scripts::Script,
    value: String,
    option: usize,
}

struct App {
    cfg: Config,
    theme: config::Theme,
    icons: icons::Cache,
    font: Font<'static>,
    apps: Vec<DesktopApp>,
    files: items::FileIndex,
    scripts: Vec<scripts::Script>,
    scripts_sig: u64,
    items: Vec<Item>,
    query: String,
    edit: input::Edit,
    caret_visible: bool,
    caret_next: Instant,
    sel: usize,
    top: usize,
    /// Last wheel or keyboard scroll; the scrollbar thumb shows for a second
    /// after it, as zlaunch's auto-hiding scrollbar does.
    last_scroll: Option<Instant>,
    shown: bool,
    configured: bool,
    cur_h: u32,
    scale: u32,
    outputs: Vec<Out>,
    repeat_rate: i32,
    repeat_delay: i32,
    repeating: Option<(u32, Instant)>,
    /// When the keyboard left the panel. It hides if focus has not come back shortly after.
    left_at: Option<Instant>,
    /// Last pointer position on the panel, in surface pixels. Hover never moves the selection.
    pointer: Option<(f64, f64)>,
    pointer_on_backdrop: bool,
    last_click: Option<(usize, Instant)>,
    /// The combined-view query to restore on the back arrow, saved when a
    /// sub-view (emoji, clipboard) is entered from the main list.
    origin: Option<String>,
    input_drag: bool,
    input_click: Option<(Instant, u8, f64, f64)>,
    settings_error: Option<String>,
    hist: hist::Hist,
    panel: Option<(String, Vec<Item>, usize)>,
    panel_filter: String,
    detail: bool,
    /// A picker selection is previewing a theme that is not saved yet.
    previewing: bool,
    script_form: Option<ScriptForm>,
    script_run: Option<scripts::Run>,
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
        let vp = if self.query.starts_with(':') {
            self.emoji_rows() as f32 * ui::EMOJI_CELL
        } else {
            ui::viewport(&self.items, self.cfg.max_rows)
        };
        if self.panel.is_some() {
            vp.max(actions_ui::MIN_VIEWPORT)
        } else if self.detail {
            vp.max(300.0)
        } else {
            vp
        }
    }

    fn panel_height(&self) -> u32 {
        if self.panel.is_some() || self.detail {
            (ui::HEAD + self.vp() + 2.0 * ui::LIST_PAD) as u32
        } else if self.query.starts_with(':') {
            ui::emoji_panel_height(self.emoji_rows())
        } else {
            ui::panel_height(&self.items, self.cfg.max_rows)
        }
    }

    fn refresh(&mut self) {
        // Typing away the sub-view prefix also leaves the sub-view.
        if self.origin.is_some() && !self.query.starts_with([':', ';', '#']) {
            self.origin = None;
        }
        if self.query.starts_with('/') {
            self.files
                .load(&self.cfg.file_roots, &self.cfg.file_excludes);
        }
        self.edit.sync(&self.query);
        if let Some(form) = &self.script_form {
            self.items = vec![Item {
                title: form.script.arg.clone(),
                sub: if form.script.arg_secret {
                    "Password input".into()
                } else {
                    form.value.clone()
                },
                tag: "Script",
                icon: None,
                action: Action::ScriptSubmit,
                best: false,
            }];
            return;
        }
        if let Some(run) = &self.script_run {
            self.items = scripts::output_items(run);
            return;
        }
        let s = Search {
            cfg: &self.cfg,
            apps: &self.apps,
            files: self.files.files(),
            scripts: &self.scripts,
            history: &self.clip,
        };
        self.items = s.run(&self.query);
        items::rank_history(&mut self.items, &self.query, &self.cfg, &self.hist);
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
        let before = self.top;
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
        if self.top != before {
            self.last_scroll = Some(Instant::now());
        }
    }

    fn pause_caret(&mut self) {
        self.caret_visible = true;
        self.caret_next = Instant::now() + Duration::from_millis(300);
    }

    fn show(&mut self, qh: &QueueHandle<App>) {
        self.pause_caret();
        if self.shown {
            return;
        }
        if self.compositor.is_none() || self.shell.is_none() {
            return;
        }
        self.apps = items::scan_apps(&self.cfg);
        let sig = scripts::scripts_sig(&self.cfg);
        if sig != self.scripts_sig {
            self.scripts = scripts::scan_scripts(&self.cfg);
            self.scripts_sig = sig;
        }
        for script in &mut self.scripts {
            if script.mode == scripts::ScriptMode::Inline {
                if let Some(line) = scripts::inline_cached(&script.exec) {
                    script.comment = line;
                }
            }
        }
        scripts::refresh_inline(&self.scripts);
        self.last_click = None;
        self.pointer = None;
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
        self.cur_h = self.panel_height();
        let root = if let Some(sub) = &self.subcompositor {
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
        let l = sh.get_layer_surface(&root, out, Layer::Overlay, "zl".into(), qh, ());
        if self.backdrop.is_some() {
            l.set_size(0, 0);
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
        self.files.clear();
        self.script_form = None;
        self.script_run = None;
        self.repeating = None;
        self.left_at = None;
        self.close_panel();
        self.pointer_on_backdrop = false;
        self.pointer = None;
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
        self.ensure_visible();
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
        self.icons.ensure_input(s);
        if let Some(b) = &self.backdrop {
            let (x, y) = panel_position(b.width, b.height, self.cfg.width, h);
            b.panel.set_position(x, y);
            b.surface.commit();
        }
        let Some(surf) = &self.surface else { return };
        let mut cv = ui::Canvas::new(self.cfg.width * s, h * s);
        let display_query = if let Some(form) = &self.script_form {
            format!(
                "{}: {}",
                form.script.name,
                if form.script.arg_secret {
                    "*".repeat(form.value.chars().count())
                } else {
                    form.value.clone()
                }
            )
        } else if let Some(run) = &self.script_run {
            format!(
                "{}{}",
                run.name,
                if run.done {
                    " - Ctrl+C copy, Ctrl+R run again"
                } else {
                    " - running"
                }
            )
        } else if self.query.starts_with("confirm script-run ") {
            "Run script?".into()
        } else if self.query.starts_with("confirm script-del ") {
            "Delete script?".into()
        } else {
            self.query.clone()
        };
        let v = ui::View {
            icons: &self.icons,
            query: &display_query,
            input: (self.script_form.is_none() && self.script_run.is_none()).then_some((
                self.edit.caret,
                self.edit.anchor,
                self.caret_visible,
            )),
            items: &self.items,
            sel: self.sel,
            top: self.top,
            vp,
            theme: self.theme,
            w: self.cfg.width,
            h,
            scale: s,
            font_size: self.cfg.font_size,
            back: self.origin.is_some(),
            show_scrollbar: self
                .last_scroll
                .is_some_and(|t| t.elapsed() < Duration::from_secs(1)),
        };
        if self.query.starts_with(';') {
            clipboard_ui::draw(&mut cv, &self.font, &v, &self.clip);
        } else {
            ui::draw(&mut cv, &self.font, &v);
        }
        if self.detail {
            if let Some(text) = self
                .items
                .get(self.sel)
                .and_then(|it| details::text(it, &self.cfg, &self.apps, &self.hist))
            {
                details::draw(&mut cv, &self.font, &v, &text);
            }
        }
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
        let picked = if items::theme_view(&self.query) {
            self.items.get(self.sel).and_then(|it| match &it.action {
                Action::Theme(name) => Some(name.clone()),
                _ => None,
            })
        } else {
            None
        };
        match picked {
            Some(name) => {
                let mut cfg = self.cfg.clone();
                cfg.theme = name;
                self.theme = cfg.theme();
                self.previewing = true;
            }
            None if self.previewing && !items::theme_view(&self.query) => {
                self.theme = self.cfg.theme();
                self.previewing = false;
            }
            _ => {}
        }
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
        let Some(mut it) = picked else {
            if items::theme_view(&self.query) {
                self.query.clear();
                self.refresh();
                self.redraw(qh);
            }
            return;
        };
        if it.tag == "Action" && matches!(it.action, Action::Web(_)) {
            if let Some(source) = self.items.get(self.sel) {
                it.title = source.title.clone();
                it.tag = source.tag;
            }
        }
        let record_usage = it.tag != "Action"
            || self
                .items
                .get(self.sel)
                .is_some_and(|source| source.action == it.action);
        self.close_panel();
        if record_usage
            && matches!(
                it.action,
                Action::App(_)
                    | Action::Clip(_)
                    | Action::ClipImg(_)
                    | Action::Script(_)
                    | Action::AppArg(..)
                    | Action::AppForce(..)
            )
        {
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
            Action::Script(i) | Action::AppArg(i, _) | Action::AppForce(i, _) => {
                let arg = match &it.action {
                    Action::AppArg(_, v) => Some(v.clone()),
                    Action::AppForce(_, v) => v.clone(),
                    _ => None,
                };
                self.run_script(i, arg, matches!(it.action, Action::AppForce(..)), qh);
            }
            Action::ScriptToggle(name) => {
                if scripts::toggle(&name).is_err() {
                    self.settings_error = Some("Could not save script settings".into());
                }
                self.scripts = scripts::scan_scripts(&self.cfg);
                self.refresh();
                self.redraw(qh);
            }
            Action::ScriptSamples => {
                if scripts::samples(&self.cfg).is_err() {
                    self.settings_error = Some("Could not write sample scripts".into());
                }
                self.scripts = scripts::scan_scripts(&self.cfg);
                self.refresh();
                self.redraw(qh);
            }
            Action::ScriptEdit(path) => {
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());
                let command = format!(
                    "{} {editor} '{}'",
                    self.cfg.terminal,
                    path.replace('\'', "'\\''")
                );
                launch_application(&command);
                self.hide();
            }
            Action::ScriptDelete(path) => {
                if std::fs::remove_file(&path).is_err() {
                    self.settings_error = Some("Could not delete script".into());
                }
                self.scripts = scripts::scan_scripts(&self.cfg);
                self.query = "scripts".into();
                self.refresh();
                self.redraw(qh);
            }
            Action::ScriptSubmit => {
                if let Some(form) = self.script_form.take() {
                    self.start_script(form.script, Some(form.value), qh);
                }
            }
            Action::ScriptOutput => {
                if let Some(run) = &self.script_run {
                    let text = scripts::strip_ansi(&run.text);
                    self.set_clipboard(text.as_bytes(), false, qh);
                }
            }
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
                if self.query.starts_with(':') && emojis::get(&t).is_some() {
                    hist::emoji_add(&t);
                }
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
            Action::Trash(p) => {
                if items::trash(&p) {
                    let dir = format!("{p}/");
                    self.files.retain(|f| f != &p && !f.starts_with(&dir));
                } else {
                    self.settings_error = Some("Could not move to the trash".into());
                }
                let error = self.settings_error.take();
                self.query = "/".into();
                self.sel = 0;
                self.top = 0;
                self.refresh();
                self.settings_error = error;
                self.redraw(qh);
            }
            Action::Nothing => {}
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
            Action::Setting(key) => self.change_setting(&key, 1, qh),
            Action::Theme(name) => {
                if settings::set_theme(&mut self.cfg, &name) {
                    // The saved theme stays on; back to the root list, like zlaunch.
                    self.previewing = false;
                    self.theme = self.cfg.theme();
                    self.query.clear();
                } else {
                    self.settings_error = Some("Could not save settings".into());
                }
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
            Action::Details => {
                self.detail = !self.detail;
                self.redraw(qh);
            }
            Action::Web(url) => {
                if items::link_fill(&url, "").is_some() {
                    if let Some(link) = self
                        .cfg
                        .link
                        .iter()
                        .find(|l| l.name == it.title && l.url == url && !l.alias.is_empty())
                    {
                        self.query = format!("{} ", link.alias);
                        self.refresh();
                        self.redraw(qh);
                    }
                    return;
                }
                self.hist.record(&format!("Link:{}", it.title));
                self.hide();
                let opener = self
                    .cfg
                    .link
                    .iter()
                    .find(|l| l.name == it.title && !l.app.is_empty())
                    .and_then(|l| self.apps.iter().find(|a| a.name == l.app));
                if let Some(a) = opener {
                    let mut cmd = items::exec_open(&a.exec, &url);
                    if a.terminal {
                        cmd = format!("{} {cmd}", self.cfg.terminal);
                    }
                    launch_application(&cmd);
                } else {
                    let _ = std::process::Command::new("xdg-open")
                        .arg(&url)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn();
                }
            }
            Action::PinToggle(key) => {
                self.hist.toggle_pin(&key);
                self.refresh();
                self.redraw(qh);
            }
            Action::SetToneFor(name, tone) => {
                if !hist::emoji_tone_set(&name, tone) {
                    self.settings_error = Some("Could not save emoji skin tone".into());
                }
                let sel = self.sel;
                self.refresh();
                self.sel = sel.min(self.items.len().saturating_sub(1));
                self.redraw(qh);
            }
            Action::EmojiReset(sequence) => {
                if let Some(e) = emojis::get(&sequence) {
                    self.hist
                        .reset(&format!("Emoji:{}", items::emoji_base(e).name()));
                }
                hist::emoji_reset(&sequence);
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
        let mut acts = items::actions_for(it, self.hist.is_pinned(&items::key_of(it)), &self.cfg);
        let script = match &it.action {
            Action::Script(i) | Action::AppArg(i, _) => self.scripts.get(*i),
            Action::ScriptToggle(name) => self.scripts.iter().find(|s| &s.name == name),
            _ => None,
        };
        if let Some(script) = script {
            let path = script.exec.trim_matches('\'').replace("'\\''", "'");
            for (title, action) in [
                ("Edit script", Action::ScriptEdit(path.clone())),
                (
                    "Delete script",
                    Action::View(format!("confirm script-del {path}")),
                ),
            ] {
                acts.push(Item {
                    title: title.into(),
                    sub: script.name.clone(),
                    tag: "Action",
                    icon: None,
                    action,
                    best: false,
                });
            }
        }
        self.panel = Some((it.title.clone(), acts, 0));
        self.panel_filter.clear();
    }

    fn run_script(&mut self, i: usize, arg: Option<String>, force: bool, qh: &QueueHandle<App>) {
        let Some(script) = self.scripts.get(i).cloned() else {
            return;
        };
        if scripts::disabled().contains(&format!("cmd:{}", script.name)) {
            return;
        }
        if script.confirm && !force {
            self.query = format!(
                "confirm script-run {i}\t{}\t{}",
                script.name,
                arg.unwrap_or_default()
            );
            self.refresh();
            self.redraw(qh);
            return;
        }
        if arg.is_none() && !script.arg.is_empty() {
            self.script_form = Some(ScriptForm {
                value: script.arg_opts.first().cloned().unwrap_or_default(),
                option: 0,
                script,
            });
            self.refresh();
            self.redraw(qh);
            return;
        }
        self.start_script(script, arg, qh);
    }

    fn start_script(
        &mut self,
        script: scripts::Script,
        arg: Option<String>,
        qh: &QueueHandle<App>,
    ) {
        self.close_panel();
        let cmd = scripts::command(&script, arg.as_deref());
        self.script_form = None;
        if matches!(
            script.mode,
            scripts::ScriptMode::Terminal
                | scripts::ScriptMode::Silent
                | scripts::ScriptMode::Inline
        ) {
            let cmd = if script.terminal {
                format!(
                    "{} sh -c '{}'",
                    term_prefix(&self.cfg.terminal, true),
                    cmd.replace('\'', "'\\''")
                )
            } else {
                cmd
            };
            if script.terminal && self.cfg.terminal.is_empty() {
                self.settings_error =
                    Some("Set terminal in config before running terminal scripts".into());
                self.refresh();
                self.redraw(qh);
                return;
            }
            launch_application(&cmd);
            scripts::script_ran(&script.name);
            self.hide();
            return;
        }
        match scripts::Run::start(script.name, cmd, script.mode == scripts::ScriptMode::Hud) {
            Ok(run) => self.script_run = Some(run),
            Err(error) => self.settings_error = Some(format!("Could not start script: {error}")),
        }
        self.sel = 0;
        self.top = 0;
        self.refresh();
        self.redraw(qh);
    }

    fn pump_script(&mut self, qh: &QueueHandle<App>) {
        let changed = self.script_run.as_mut().is_some_and(|run| run.pump());
        if self.script_run.as_ref().is_some_and(|run| {
            run.toast
                && run
                    .finished
                    .is_some_and(|at| at.elapsed() > Duration::from_secs(3))
        }) {
            self.hide();
            return;
        }
        if changed {
            self.refresh();
            self.redraw(qh);
        }
    }

    fn mv(&mut self, d: isize) {
        let n = self.items.len() as isize;
        if n > 0 {
            self.sel = (self.sel as isize + d).clamp(0, n - 1) as usize;
        }
    }

    fn key(&mut self, key: u32, qh: &QueueHandle<App>) -> bool {
        self.pause_caret();
        use xkbcommon::xkb::keysyms as k;
        let Some(st) = &self.xkb else { return false };
        let code = xkb::Keycode::new(key + 8);
        let sym = st.key_get_one_sym(code).raw();
        let mut utf8 = st.key_get_utf8(code);
        let ctrl = st.mod_name_is_active(xkb::MOD_NAME_CTRL, xkb::STATE_MODS_EFFECTIVE);
        if self.script_form.is_some() {
            if sym == k::KEY_Escape {
                self.script_form = None;
                self.query.clear();
                self.refresh();
                self.redraw(qh);
                return false;
            }
            if sym == k::KEY_Return || sym == k::KEY_KP_Enter {
                let form = self.script_form.take().unwrap();
                self.start_script(form.script, Some(form.value), qh);
                return false;
            }
            let form = self.script_form.as_mut().unwrap();
            if !form.script.arg_opts.is_empty() {
                if matches!(sym, k::KEY_Down | k::KEY_Tab | k::KEY_Up) {
                    let n = form.script.arg_opts.len();
                    form.option = if sym == k::KEY_Up {
                        (form.option + n - 1) % n
                    } else {
                        (form.option + 1) % n
                    };
                    form.value = form.script.arg_opts[form.option].clone();
                }
            } else if sym == k::KEY_BackSpace {
                form.value.pop();
            } else if ctrl && utf8 == "\u{15}" {
                form.value.clear();
            } else if !ctrl && !utf8.chars().any(|c| c.is_control()) {
                form.value.push_str(&utf8);
            }
            self.refresh();
            self.redraw(qh);
            return true;
        }
        if self.script_run.is_some() {
            if sym == k::KEY_Escape {
                self.script_run = None;
                self.query.clear();
                self.refresh();
                self.redraw(qh);
                return false;
            }
            if ctrl && utf8 == "\u{3}" {
                let text = scripts::strip_ansi(&self.script_run.as_ref().unwrap().text);
                self.set_clipboard(text.as_bytes(), false, qh);
                return false;
            }
            if ctrl && utf8 == "\u{12}" {
                let run = self.script_run.take().unwrap();
                let (name, cmd, toast) = (run.name.clone(), run.cmd.clone(), run.toast);
                drop(run);
                match scripts::Run::start(name, cmd, toast) {
                    Ok(run) => self.script_run = Some(run),
                    Err(e) => self.settings_error = Some(format!("Could not start script: {e}")),
                };
                self.refresh();
                self.redraw(qh);
                return false;
            }
            if matches!(
                sym,
                k::KEY_Down | k::KEY_Page_Down | k::KEY_Up | k::KEY_Page_Up
            ) {
                self.mv(if matches!(sym, k::KEY_Up | k::KEY_Page_Up) {
                    -1
                } else {
                    1
                });
                self.redraw(qh);
            }
            return false;
        }
        if ctrl && matches!(utf8.as_str(), "\u{5}" | "\u{4}") && self.query.starts_with("scripts") {
            if let Some(Action::ScriptToggle(name)) = self.items.get(self.sel).map(|i| &i.action) {
                if let Some(script) = self.scripts.iter().find(|s| &s.name == name) {
                    let path = script.exec.trim_matches('\'').replace("'\\''", "'");
                    if utf8 == "\u{5}" {
                        self.items[self.sel].action = Action::ScriptEdit(path);
                        self.activate(qh);
                    } else {
                        self.query = format!("confirm script-del {path}");
                        self.refresh();
                        self.redraw(qh);
                    }
                }
            }
            return false;
        }
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
        if ctrl && sym == k::KEY_o {
            self.detail = !self.detail;
            self.redraw(qh);
            return false;
        }
        if ctrl && sym == k::KEY_b {
            self.open_panel();
            self.redraw(qh);
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
        let (columns, grid) = (self.columns(), self.query.starts_with(':'));
        let selected = self.items.get(self.sel).map(|i| i.action.clone());
        let effect = keys::handle(
            &mut keys::KeyState {
                query: &mut self.query,
                edit: &mut self.edit,
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
            keys::Effect::Moved => {}
            keys::Effect::Copy(text) => {
                if !text.is_empty() {
                    copy_input_text(&text);
                }
            }
            keys::Effect::Cut(text) => {
                if !text.is_empty() && copy_input_text(&text) {
                    self.edit.replace(&mut self.query, "");
                    self.refresh();
                }
            }
            keys::Effect::Paste => {
                if let Ok(out) = std::process::Command::new("timeout")
                    .args(["1", "wl-paste", "--no-newline", "--type", "text"])
                    .output()
                {
                    if out.status.success() {
                        let text: String = String::from_utf8_lossy(&out.stdout)
                            .chars()
                            .map(|c| if c.is_control() { ' ' } else { c })
                            .collect();
                        self.edit.replace(&mut self.query, &text);
                        self.refresh();
                    }
                }
            }
            keys::Effect::Step(name, dir) => {
                self.change_setting(&name, dir, qh);
                return true;
            }
            keys::Effect::DeleteClip => {
                let position = self.sel;
                if let Some(action) = selected {
                    if self.clip.remove(&action) && self.cfg.clipboard_persist {
                        self.clip.save();
                    }
                }
                self.refresh();
                self.sel = position.min(self.items.len().saturating_sub(1));
            }
        }
        self.redraw(qh);
        repeat
    }

    fn row_at(&self, x: f64, y: f64) -> Option<usize> {
        if self.query.starts_with(';') && x >= self.cfg.width as f64 * 0.5 {
            return None;
        }
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

fn term_prefix(term: &str, hold: bool) -> String {
    let known = ["foot", "kitty", "alacritty"];
    if hold && known.iter().any(|k| term.starts_with(k)) {
        format!("{term} --hold")
    } else {
        term.to_string()
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
                "wl_subcompositor" => a.subcompositor = Some(r.bind(name, 1, qh, ())),
                "wp_viewporter" => a.viewporter = Some(r.bind(name, 1, qh, ())),
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
                if a.shown && a.cfg.close_on_focus_loss && current == Some(&surface) {
                    a.left_at = Some(Instant::now());
                }
            }
            Event::Enter { .. } => a.left_at = None,
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
                if a.pointer.is_some() && a.panel.is_some() {
                    if let Some(row) = a.panel_row_at(surface_x, surface_y) {
                        if let Some(p) = &mut a.panel {
                            p.2 = row;
                        }
                        a.redraw(qh);
                    }
                }
                if a.pointer.is_some() && a.input_drag {
                    let at = ui::input_at(&a.font, &a.query, a.edit.caret, a.cfg.width, surface_x);
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
                if a.pointer.is_some() && a.panel.is_some() {
                    if let Some(row) = a.panel_row_at(surface_x, surface_y) {
                        if let Some(p) = &mut a.panel {
                            p.2 = row;
                        }
                        a.redraw(qh);
                    }
                }
                if a.pointer.is_some() && a.input_drag {
                    let at = ui::input_at(&a.font, &a.query, a.edit.caret, a.cfg.width, surface_x);
                    a.edit.move_to(&a.query, at, true);
                    a.redraw(qh);
                }
            }
            Event::Leave { .. } => {
                a.pointer = None;
                a.input_drag = false;
                a.pointer_on_backdrop = false;
            }
            Event::Button {
                button: 272,
                state: WEnum::Value(ButtonState::Pressed),
                ..
            } => {
                if a.shown && a.pointer_on_backdrop {
                    a.hide();
                    return;
                }
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

                // The back arrow returns to the combined view, restoring the
                // query that was there when the sub-view was entered.
                if a.shown
                    && a.origin.is_some()
                    && a.pointer.is_some_and(|(x, y)| ui::back_button_at(x, y))
                {
                    a.query = a.origin.take().unwrap_or_default();
                    a.last_click = None;
                    a.refresh();
                    a.redraw(qh);
                    return;
                }
                if a.shown
                    && a.pointer
                        .is_some_and(|(x, y)| ui::clear_button_at(a.cfg.width, &a.query, x, y))
                {
                    a.query.truncate(usize::from(items::theme_view(&a.query)));
                    a.last_click = None;
                    a.refresh();
                    a.redraw(qh);
                    return;
                }
                if let Some((x, y)) = a.pointer.filter(|(_, y)| *y >= 0.0 && *y < ui::HEAD as f64) {
                    let _ = y;
                    a.edit.sync(&a.query);
                    let at = ui::input_at(&a.font, &a.query, a.edit.caret, a.cfg.width, x);
                    let now = Instant::now();
                    let count = a
                        .input_click
                        .filter(|(t, _, px, py)| {
                            now.duration_since(*t) < Duration::from_millis(400)
                                && (x - px).abs() < 4.0
                                && (y - py).abs() < 4.0
                        })
                        .map_or(1, |(_, n, _, _)| (n % 3) + 1);
                    a.input_click = Some((now, count, x, y));
                    if count == 3 {
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
            Event::Button {
                button: 272,
                state: WEnum::Value(ButtonState::Released),
                ..
            } => a.input_drag = false,
            Event::Axis {
                axis: WEnum::Value(Axis::VerticalScroll),
                value,
                ..
            } => {
                if a.pointer_on_backdrop {
                    return;
                }
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
                a.last_scroll = Some(Instant::now());
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
        scripts: scripts::scan_scripts(&cfg),
        scripts_sig: scripts::scripts_sig(&cfg),
        items: vec![],
        query: String::new(),
        edit: input::Edit::default(),
        caret_visible: true,
        caret_next: Instant::now(),
        sel: 0,
        top: 0,
        last_scroll: None,
        shown: false,
        configured: false,
        cur_h: 0,
        scale: 1,
        outputs: vec![],
        repeat_rate: 25,
        repeat_delay: 400,
        repeating: None,
        left_at: None,
        pointer: None,
        pointer_on_backdrop: false,
        last_click: None,
        origin: None,
        input_drag: false,
        input_click: None,
        settings_error: None,
        hist: hist::Hist::load(),
        panel: None,
        panel_filter: String::new(),
        detail: false,
        previewing: false,
        script_form: None,
        script_run: None,
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
        if app.shown {
            let wait = app
                .caret_next
                .saturating_duration_since(Instant::now())
                .as_millis() as i32
                + 1;
            timeout = if timeout < 0 { wait } else { timeout.min(wait) };
        }
        if app.script_run.is_some() {
            timeout = if timeout < 0 { 50 } else { timeout.min(50) };
        }
        if !app.reads.is_empty() {
            timeout = if timeout < 0 { 500 } else { timeout.min(500) };
        }
        if let Some(t) = app.left_at {
            let wait = FOCUS_GRACE.saturating_sub(t.elapsed()).as_millis() as i32 + 1;
            timeout = if timeout < 0 { wait } else { timeout.min(wait) };
        }
        unsafe {
            libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout);
        }
        app.finish_reads(&qh);
        if app.shown && Instant::now() >= app.caret_next {
            app.caret_visible = !app.caret_visible;
            app.caret_next = Instant::now() + Duration::from_millis(500);
            app.redraw(&qh);
        }
        app.pump_script(&qh);
        if fds[0].revents & libc::POLLIN != 0 {
            let _ = guard.read();
        } else {
            drop(guard);
        }
        if app.left_at.is_some_and(|t| t.elapsed() >= FOCUS_GRACE) {
            app.left_at = None;
            if app.shown {
                app.hide();
            }
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

#[cfg(test)]
mod backdrop_tests {
    use super::*;

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
            icons: icons::Cache::new(&cfg.icon_theme, &cfg.emoji_font),
            font: load_font(&cfg),
            apps: items::scan_apps(&cfg),
            items: vec![],
            query: String::new(),
            edit: input::Edit::default(),
            caret_visible: true,
            caret_next: Instant::now(),
            sel: 0,
            top: 0,
            last_scroll: None,
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
            pointer_on_backdrop: false,
            origin: None,
            input_drag: false,
            input_click: None,
            left_at: None,
            settings_error: None,
            previewing: false,
            hist: hist::Hist::default(),
            panel: None,
            panel_filter: String::new(),
            detail: false,
            scripts: vec![],
            scripts_sig: 0,
            script_form: None,
            script_run: None,
            files: items::FileIndex::default(),
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
    #[test]
    fn preview_never_changes_config_and_leaving_restores_it() {
        let cfg = Config {
            theme: "nord".into(),
            ..Config::default()
        };
        let mut a = app(cfg);
        a.query = "#dracula".into();
        a.refresh();
        a.sync_theme_preview();
        assert_eq!(a.cfg.theme, "nord");
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
}
