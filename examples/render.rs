//! Render the launcher canvas without a Wayland connection.
//!
//! The panel is translucent, so by default the PNG is composited over a dark
//! backdrop, close to how it looks over a dark desktop. `--backdrop none`
//! writes the raw transparent PNG and `--backdrop '#rrggbb'` picks the colour.
#![allow(dead_code)]
#[path = "../src/app_cache.rs"]
mod app_cache;
#[path = "../src/calculator.rs"]
mod calculator;
#[path = "../src/clip.rs"]
mod clip;
#[path = "../src/clipboard_ui.rs"]
mod clipboard_ui;
#[path = "../src/config.rs"]
mod config;
#[allow(dead_code)]
#[path = "../src/dates.rs"]
mod dates;
#[allow(dead_code)]
#[path = "../src/hist.rs"]
mod hist;
#[path = "../src/icons.rs"]
mod icons;
#[path = "../src/input.rs"]
mod input;
#[path = "../src/items.rs"]
mod items;
#[path = "../src/scorer.rs"]
#[allow(dead_code)]
mod scorer;
#[path = "../src/scroll.rs"]
mod scroll;
#[path = "../src/system_actions.rs"]
mod system_actions;
#[cfg(test)]
#[path = "../src/testutil.rs"]
mod testutil;
#[path = "../src/ui.rs"]
mod ui;
#[path = "../src/web_search.rs"]
mod web_search;
#[path = "../src/windows.rs"]
#[allow(dead_code)]
mod windows;

use std::{error::Error, fs::File, io::BufWriter};

struct Options {
    output: String,
    view: String,
    query: String,
    cfg: config::Config,
    scale: u32,
    selected: usize,
    top: usize,
    backdrop: Option<config::Rgb>,
    back: bool,
    scrollbar: bool,
    caret: Option<usize>,
    anchor: Option<usize>,
    caret_visible: bool,
}

impl Options {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, Box<dyn Error>> {
        let mut o = Self {
            output: "render.png".into(),
            view: "launcher".into(),
            query: String::new(),
            cfg: config::Config {
                clipboard_persist: false,
                ..Default::default()
            },
            scale: 1,
            selected: 0,
            top: 0,
            backdrop: Some([17, 17, 17]),
            back: false,
            scrollbar: false,
            caret: None,
            anchor: None,
            caret_visible: true,
        };
        let mut args = args;
        while let Some(arg) = args.next() {
            let value = args
                .next()
                .ok_or_else(|| format!("missing value for {arg}"))?;
            match arg.as_str() {
                "--output" => o.output = value,
                "--view" => o.view = value,
                "--query" => o.query = value,
                "--theme" => {
                    if config::load_theme(&value).is_none() {
                        return Err(format!("unknown theme: {value}").into());
                    }
                    o.cfg.theme = value;
                }
                "--font" => o.cfg.font_path = value,
                "--emoji-font" => o.cfg.emoji_font = value,
                "--scale" => o.scale = value.parse()?,
                "--width" => o.cfg.width = value.parse()?,
                "--rows" => o.cfg.max_rows = value.parse()?,
                "--selected" => o.selected = value.parse()?,
                "--top" => o.top = value.parse()?,
                "--back" => o.back = value.parse()?,
                "--scrollbar" => o.scrollbar = value.parse()?,
                "--caret" => o.caret = Some(value.parse()?),
                "--anchor" => o.anchor = Some(value.parse()?),
                "--caret-visible" => o.caret_visible = value.parse()?,
                "--backdrop" => {
                    o.backdrop = if value == "none" {
                        None
                    } else {
                        Some(config::hex(&value).ok_or("backdrop must be #rrggbb or none")?)
                    };
                }
                _ => return Err(format!("unknown option: {arg}").into()),
            }
        }
        if !(1..=4).contains(&o.scale)
            || !(400..=1200).contains(&o.cfg.width)
            || !(1..=20).contains(&o.cfg.max_rows)
        {
            return Err("scale must be 1..4, width 400..1200, rows 1..20".into());
        }
        o.query = match o.view.as_str() {
            "launcher" => o.query,
            "themes" => format!("#{}", o.query),
            "emoji" => format!(":{}", o.query),
            "clipboard" => format!(";{}", o.query),
            _ => return Err("view must be launcher, themes, emoji or clipboard".into()),
        };
        o.input()?;
        if o.back && !o.query.starts_with([':', ';', '#']) {
            return Err("back requires an emoji, clipboard or theme sub-view".into());
        }
        Ok(o)
    }

    fn input(&self) -> Result<(usize, Option<usize>, bool), Box<dyn Error>> {
        let visible = self
            .query
            .strip_prefix([':', ';', '#'])
            .map(str::trim_start)
            .unwrap_or(&self.query);
        let prefix = self.query.len() - visible.len();
        let offset = |value: usize| -> Result<usize, Box<dyn Error>> {
            if value > visible.len() || !visible.is_char_boundary(value) {
                return Err(
                    "caret and anchor must be UTF-8 byte boundaries in visible query".into(),
                );
            }
            Ok(prefix + value)
        };
        Ok((
            offset(self.caret.unwrap_or(visible.len()))?,
            self.anchor.map(offset).transpose()?,
            self.caret_visible,
        ))
    }
}

fn fixture() -> Vec<items::App> {
    let names = [
        ("Firefox", "Web Browser"),
        ("Files", "File Manager"),
        ("Terminal", "Terminal Emulator"),
        ("Visual Studio Code", "Code Editor"),
        ("Spotify", "Music Player"),
        ("Discord", "Voice and Text Chat"),
        ("Calculator", "Calculator"),
        ("Settings", "System Settings"),
    ];
    (0..600)
        .map(|i| {
            let (name, comment) = names[i % names.len()];
            items::App {
                name: if i < names.len() {
                    name.into()
                } else {
                    format!("{name} {i:03}")
                },
                comment: comment.into(),
                exec: "true".into(),
                icon: String::new(),
                terminal: false,
            }
        })
        .collect()
}

fn rgba(canvas: &ui::Canvas) -> Vec<u8> {
    canvas
        .buf
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let a = u32::from(p[3]);
            let straight = |c: u8| {
                (u32::from(c) * 255 + a / 2)
                    .checked_div(a)
                    .unwrap_or(0)
                    .min(255) as u8
            };
            [straight(p[2]), straight(p[1]), straight(p[0]), p[3]]
        })
        .collect()
}

/// Opaque image: the premultiplied canvas over a solid backdrop.
fn composite(canvas: &ui::Canvas, backdrop: config::Rgb) -> Vec<u8> {
    canvas
        .buf
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let a = u32::from(p[3]);
            let over =
                |c: u8, b: u8| (u32::from(c) + u32::from(b) * (255 - a) / 255).min(255) as u8;
            [
                over(p[2], backdrop[0]),
                over(p[1], backdrop[1]),
                over(p[0], backdrop[2]),
                255,
            ]
        })
        .collect()
}

fn render(o: &Options) -> Result<(ui::Canvas, usize), Box<dyn Error>> {
    let font_path = o.cfg.font_file(false).ok_or("could not resolve font")?;
    let font = rusttype::Font::try_from_vec(std::fs::read(font_path)?)
        .ok_or("could not load font; use --font /path/to/font.ttf")?;
    let bold_font = o
        .cfg
        .font_file(true)
        .and_then(|path| std::fs::read(path).ok())
        .and_then(rusttype::Font::try_from_vec);
    let startup_fonts = o.cfg.startup_fonts();
    let apps = fixture();
    let mut history = clip::History::new(100);
    let now = clip::now();
    for (i, text) in [
        "https://github.com/zortax/zlaunch",
        "cargo build --release",
        "Meeting notes\nReview launcher layout and keyboard shortcuts.",
        "The quick brown fox jumps over the lazy dog.",
    ]
    .iter()
    .enumerate()
    {
        history.entries.push(clip::Clip {
            text: (*text).into(),
            at: now - i as u64 * 70,
            files: false,
        });
    }
    let mut image = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut image, 240, 140);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let pixels: Vec<_> = (0..140)
            .flat_map(|y| (0..240).flat_map(move |x| [x as u8, y as u8, 180, 255]))
            .collect();
        encoder.write_header()?.write_image_data(&pixels)?;
    }
    history.push_image(&image);
    history.images[0].at = now - 30;
    let items = items::Search {
        cfg: &o.cfg,
        apps: &apps,
        windows: &[],
        history: &history,
    }
    .run(&o.query);
    let sel = o.selected.min(items.len().saturating_sub(1));
    // The picker recolours the panel with the selected theme, exactly like the daemon.
    let theme = match items.get(sel).map(|it| &it.action) {
        Some(items::Action::Theme(name)) => {
            let mut cfg = o.cfg.clone();
            cfg.theme = name.clone();
            cfg.theme()
        }
        _ => o.cfg.theme(),
    };
    let layout = ui::Layout::new(&theme.metrics);
    let emoji = o.view == "emoji";
    let columns = if emoji {
        layout.emoji_columns(o.cfg.width)
    } else {
        1
    };
    let rows = layout.emoji_rows(items.len(), o.cfg.width);
    let vp = ui::LIST_VP;
    let top = if emoji {
        (o.top / columns).min(items.len().div_ceil(columns).saturating_sub(rows)) * columns
    } else {
        o.top.min(layout.max_top(&items, vp))
    };
    // zlaunch fixes every panel at launcher_size (default 600x400).
    let h = ui::PANEL_HEIGHT;
    let mut icons = icons::Cache::new("", &o.cfg.emoji_font);
    for item in items
        .iter()
        .skip(top)
        .take(if emoji { rows * columns } else { items.len() })
    {
        if let Some(name) = item.icon.as_deref().filter(|n| !n.is_empty()) {
            icons.ensure(
                name,
                if emoji {
                    (theme.metrics.emoji_font_size * o.scale as f32)
                        .round()
                        .max(1.0) as u32
                } else {
                    ui::icon_px(&theme, o.scale)
                },
            );
        }
    }
    icons.ensure_input(o.scale);
    if o.view == "clipboard" && !o.back {
        icons.ensure("phosphor:copy", 16 * o.scale);
    }
    let mut canvas = ui::Canvas::new(o.cfg.width * o.scale, h * o.scale);
    let view = ui::View {
        theme_palettes: None,
        placeholder: None,
        literal_prefix: false,
        query: &o.query,
        input_text: None,
        marked: None,
        input: Some(o.input()?),
        items: &items,
        sel,
        top,
        scroll_remainder: 0.0,
        vp,
        theme,
        w: o.cfg.width,
        h,
        scale: o.scale,
        font_size: o.cfg.startup_font_size(),
        mono_font_family: startup_fonts.mono_font_family.as_deref(),
        icons: &icons,
        bold_font: bold_font.as_ref(),
        back: o.back,
        scrollbar: if o.scrollbar {
            crate::scroll::Style::visible()
        } else {
            crate::scroll::Style::default()
        },
    };
    if o.view == "clipboard" {
        clipboard_ui::draw(&mut canvas, &font, &view, &history);
    } else {
        ui::draw(&mut canvas, &font, &view);
    }

    Ok((canvas, items.len()))
}

fn run() -> Result<(), Box<dyn Error>> {
    let o = Options::parse(std::env::args().skip(1))?;
    let (canvas, result_count) = render(&o)?;
    let pixels = match o.backdrop {
        Some(bg) => composite(&canvas, bg),
        None => rgba(&canvas),
    };
    let mut encoder =
        png::Encoder::new(BufWriter::new(File::create(&o.output)?), canvas.w, canvas.h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&pixels)?;
    println!(
        "{}: {}x{}, 600 fixture apps, {} results, no compositor",
        o.output, canvas.w, canvas.h, result_count
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("render: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_state_matches_visible_query_and_mode_origin() {
        let parse = |args: &[&str]| Options::parse(args.iter().map(|arg| (*arg).into()));
        let idle = parse(&[]).unwrap();
        assert!(!idle.back && !idle.scrollbar);
        assert_eq!(idle.input().unwrap(), (0, None, true));
        let editing = parse(&[
            "--view",
            "themes",
            "--query",
            "éclair",
            "--caret",
            "2",
            "--anchor",
            "7",
            "--back",
            "true",
            "--scrollbar",
            "true",
            "--caret-visible",
            "false",
        ])
        .unwrap();
        assert!(editing.back && editing.scrollbar);
        assert_eq!(editing.input().unwrap(), (3, Some(8), false));
        let emoji = parse(&["--view", "emoji", "--query", "  smile"]).unwrap();
        assert_eq!(emoji.input().unwrap(), (8, None, true));
        for args in [
            vec!["--query", "é", "--caret", "1"],
            vec!["--query", "x", "--anchor", "2"],
            vec!["--back", "true"],
            vec!["--scrollbar", "maybe"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn state_options_reach_the_real_canvas() {
        let parse = |args: &[&str]| Options::parse(args.iter().map(|arg| (*arg).into())).unwrap();
        let render = |o: &Options| super::render(o).map(|(canvas, _)| canvas);
        let changed = |a: &ui::Canvas, b: &ui::Canvas, region: (u32, u32, u32, u32)| {
            assert_eq!((a.w, a.h), (b.w, b.h));
            let mut count = 0;
            for y in 0..a.h {
                for x in 0..a.w {
                    let p = ((y * a.w + x) * 4) as usize;
                    if a.buf[p..p + 4] != b.buf[p..p + 4] {
                        assert!(
                            x >= region.0 && y >= region.1 && x < region.2 && y < region.3,
                            "unexpected pixel change at {x},{y}"
                        );
                        count += 1;
                    }
                }
            }
            assert!(count > 0);
        };
        let direct = render(&parse(&["--view", "themes", "--query", "nord"])).unwrap();
        let back = render(&parse(&[
            "--view", "themes", "--query", "nord", "--back", "true",
        ]))
        .unwrap();
        changed(&direct, &back, (22, 20, 39, 37));
        let idle = render(&parse(&["--query", "fire"])).unwrap();
        let scrolling = render(&parse(&["--query", "fire", "--scrollbar", "true"])).unwrap();
        changed(&idle, &scrolling, (588, 57, 596, idle.h));
        let hidden = render(&parse(&["--query", "fire", "--caret-visible", "false"])).unwrap();
        changed(&idle, &hidden, (49, 20, 140, 40));
        let selection = render(&parse(&[
            "--query", "fire", "--caret", "1", "--anchor", "3",
        ]))
        .unwrap();
        changed(&idle, &selection, (49, 20, 140, 40));
    }

    #[test]
    fn fixture_is_six_hundred_unique_apps() {
        let apps = fixture();
        assert_eq!(apps.len(), 600);
        assert_eq!(
            apps.iter()
                .map(|a| &a.name)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            600
        );
        assert_eq!(apps[0].name, "Firefox");
        assert_eq!(apps[599].name, "Settings 599");
    }

    #[test]
    fn backdrop_composites_premultiplied_pixels() {
        let mut canvas = ui::Canvas::new(2, 1);
        canvas.buf = vec![10, 10, 10, 178, 0, 0, 0, 0];
        assert_eq!(
            composite(&canvas, [17, 17, 17]),
            [15, 15, 15, 255, 17, 17, 17, 255]
        );
    }

    #[test]
    fn png_pixels_are_straight_rgba() {
        let mut canvas = ui::Canvas::new(3, 1);
        canvas.buf = vec![0, 0, 128, 128, 0, 0, 0, 0, 30, 20, 10, 255];
        assert_eq!(rgba(&canvas), [255, 0, 0, 128, 0, 0, 0, 0, 10, 20, 30, 255]);
    }

    #[test]
    fn options_reject_bad_dimensions_and_views() {
        for args in [
            ["--scale", "0"],
            ["--scale", "5"],
            ["--rows", "0"],
            ["--width", "99999"],
            ["--view", "bogus"],
            ["--theme", "bogus"],
        ] {
            assert!(Options::parse(args.into_iter().map(str::to_owned)).is_err());
        }
        assert!(Options::parse(["--output".into()].into_iter()).is_err());
        let o = Options::parse(
            ["--view", "emoji", "--query", "smile"]
                .into_iter()
                .map(str::to_owned),
        )
        .unwrap();
        assert_eq!(o.query, ":smile");
        let o = Options::parse(
            ["--view", "themes", "--query", "nord"]
                .into_iter()
                .map(str::to_owned),
        )
        .unwrap();
        assert_eq!(o.query, "#nord");
    }
}
