//! Render the launcher canvas without a Wayland connection.
//!
//! The panel is translucent, so by default the PNG is composited over a dark
//! backdrop, close to how it looks over a dark desktop. `--backdrop none`
//! writes the raw transparent PNG and `--backdrop '#rrggbb'` picks the colour.
#![allow(dead_code)]
#[path = "../src/actions_ui.rs"]
mod actions_ui;
#[path = "../src/clip.rs"]
mod clip;
#[path = "../src/clipboard_ui.rs"]
mod clipboard_ui;
#[path = "../src/config.rs"]
mod config;
#[allow(dead_code)]
#[path = "../src/dates.rs"]
mod dates;
#[path = "../src/details.rs"]
mod details;
#[allow(dead_code)]
#[path = "../src/hist.rs"]
mod hist;
#[path = "../src/icons.rs"]
mod icons;
#[path = "../src/input.rs"]
mod input;
#[path = "../src/items.rs"]
mod items;
#[path = "../src/scripts.rs"]
mod scripts;
#[path = "../src/settings.rs"]
mod settings;
#[cfg(test)]
#[path = "../src/testutil.rs"]
mod testutil;
#[path = "../src/ui.rs"]
mod ui;

use std::{error::Error, fs::File, io::BufWriter};

struct Options {
    output: String,
    view: String,
    query: String,
    cfg: config::Config,
    scale: u32,
    selected: usize,
    top: usize,
    actions: bool,
    backdrop: Option<config::Rgb>,
    detail: bool,
}

impl Options {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, Box<dyn Error>> {
        let mut o = Self {
            output: "render.png".into(),
            view: "launcher".into(),
            query: String::new(),
            cfg: config::Config {
                font: "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into(),
                terminal: String::new(),
                clipboard_persist: false,
                ..Default::default()
            },
            scale: 1,
            selected: 0,
            top: 0,
            actions: false,
            backdrop: Some([17, 17, 17]),
            detail: false,
        };
        let mut args = args;
        while let Some(arg) = args.next() {
            let value = args
                .next()
                .ok_or_else(|| format!("missing value for {arg}"))?;
            match arg.as_str() {
                "--output" => o.output = value,
                "--actions" => o.actions = value.parse()?,
                "--details" => o.detail = value.parse()?,
                "--view" => o.view = value,
                "--query" => o.query = value,
                "--theme" => {
                    if !config::THEMES.contains(&value.as_str()) {
                        return Err(format!("unknown theme: {value}").into());
                    }
                    o.cfg.theme = value;
                }
                "--font" => o.cfg.font = value,
                "--emoji-font" => o.cfg.emoji_font = value,
                "--scale" => o.scale = value.parse()?,
                "--width" => o.cfg.width = value.parse()?,
                "--rows" => o.cfg.max_rows = value.parse()?,
                "--selected" => o.selected = value.parse()?,
                "--top" => o.top = value.parse()?,
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
            "quicklinks" => format!("quicklinks {}", o.query),
            "themes" => format!("#{}", o.query),
            "scripts" => "scripts".into(),
            "script-output" => "Script output - Ctrl+C copy, Ctrl+R run again".into(),
            "emoji" => format!(":{}", o.query),
            "clipboard" => format!(";{}", o.query),
            "settings" if o.query.is_empty() => "settings".into(),
            "settings-apps" => format!("settings apps {}", o.query),
            _ => {
                return Err(
                    "view must be launcher, emoji, clipboard, settings or settings-apps".into(),
                )
            }
        };
        Ok(o)
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

fn run() -> Result<(), Box<dyn Error>> {
    let mut o = Options::parse(std::env::args().skip(1))?;
    let font = rusttype::Font::try_from_vec(std::fs::read(&o.cfg.font)?)
        .ok_or("could not load font; use --font /path/to/font.ttf")?;
    if o.view == "quicklinks" {
        o.cfg.link.push(config::Link {
            name: "Documentation".into(),
            url: "https://example.invalid/docs".into(),
            created: 1_700_000_000,
            ..Default::default()
        });
        o.cfg.link.push(config::Link {
            name: "Search documentation".into(),
            url: "https://example.invalid/?q={query}".into(),
            alias: "docs".into(),
            ..Default::default()
        });
    }
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
    let scripts: Vec<scripts::Script> = [
        (
            "Hello World",
            "Full output with ANSI colors",
            scripts::ScriptMode::Output,
        ),
        (
            "Count To",
            "Script, asks for: How far",
            scripts::ScriptMode::Output,
        ),
        ("Current Time", "18:35:12", scripts::ScriptMode::Inline),
    ]
    .into_iter()
    .map(|(name, comment, mode)| scripts::Script {
        name: name.into(),
        comment: comment.into(),
        exec: format!("'/fixture/scripts/{}.sh'", name.replace(' ', "-")),
        icon: String::new(),
        keywords: String::new(),
        terminal: false,
        mode,
        arg: if name == "Count To" {
            "How far".into()
        } else {
            String::new()
        },
        arg_opts: vec![],
        arg_secret: false,
        refresh_secs: 30,
        cwd: "/fixture/scripts".into(),
        confirm: false,
    })
    .collect();
    let items = if o.view == "script-output" {
        [
            "\x1b[32mHello from zl!\x1b[0m This is a fullOutput script.",
            "line 1",
            "line 2",
            "line 3",
        ]
        .into_iter()
        .map(|line| items::Item {
            title: line.into(),
            sub: String::new(),
            tag: "Output",
            icon: None,
            action: items::Action::ScriptOutput,
            best: false,
        })
        .collect()
    } else if o.view == "scripts" {
        scripts
            .iter()
            .enumerate()
            .map(|(i, script)| items::Item {
                title: script.name.clone(),
                sub: script.comment.clone(),
                tag: "Script",
                icon: None,
                action: items::Action::Script(i),
                best: false,
            })
            .collect()
    } else {
        items::Search {
            cfg: &o.cfg,
            apps: &apps,
            scripts: &[],
            files: &[],
            history: &history,
        }
        .run(&o.query)
    };
    let emoji = o.view == "emoji";
    let columns = if emoji {
        ui::emoji_columns(o.cfg.width)
    } else {
        1
    };
    let rows = ui::emoji_rows(items.len(), o.cfg.width, o.cfg.max_rows);
    let mut vp = if emoji {
        rows as f32 * ui::EMOJI_CELL
    } else {
        let vp = ui::viewport(&items, o.cfg.max_rows);
        if o.detail {
            vp.max(300.0)
        } else {
            vp
        }
    };
    if o.actions {
        vp = vp.max(actions_ui::MIN_VIEWPORT);
    }
    let top = if emoji {
        (o.top / columns).min(items.len().div_ceil(columns).saturating_sub(rows)) * columns
    } else {
        o.top.min(ui::max_top(&items, vp))
    };
    let sel = o.selected.min(items.len().saturating_sub(1));
    let h = if o.actions || o.detail {
        (ui::HEAD + vp + 2.0 * ui::LIST_PAD) as u32
    } else if emoji {
        ui::emoji_panel_height(rows)
    } else {
        if o.detail {
            (ui::HEAD + vp + 2.0 * ui::LIST_PAD) as u32
        } else {
            ui::panel_height(&items, o.cfg.max_rows)
        }
    };
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
                    ui::emoji_icon_px(o.scale)
                } else {
                    ui::icon_px(o.scale)
                },
            );
        }
    }
    icons.ensure_input(o.scale);
    let mut canvas = ui::Canvas::new(o.cfg.width * o.scale, h * o.scale);
    // The picker recolours the panel with the selected theme, exactly like the daemon.
    let theme = match items.get(sel).map(|it| &it.action) {
        Some(items::Action::Theme(name)) => {
            let mut cfg = o.cfg.clone();
            cfg.theme = name.clone();
            cfg.theme()
        }
        _ => o.cfg.theme(),
    };
    let view = ui::View {
        query: &o.query,
        input: None,
        items: &items,
        sel,
        top,
        vp,
        theme,
        w: o.cfg.width,
        h,
        scale: o.scale,
        font_size: o.cfg.font_size,
        icons: &icons,
        back: false,
        show_scrollbar: true,
    };
    if o.view == "clipboard" {
        clipboard_ui::draw(&mut canvas, &font, &view, &history);
    } else {
        ui::draw(&mut canvas, &font, &view);
    }
    if o.detail {
        if let Some(text) = items
            .get(sel)
            .and_then(|it| details::text(it, &o.cfg, &apps, &hist::Hist::default()))
        {
            details::draw(&mut canvas, &font, &view, &text);
        }
    }
    if o.actions {
        if let Some(it) = items.get(sel) {
            let actions = items::actions_for(it, false, &o.cfg);
            actions_ui::draw(
                &mut canvas,
                &font,
                &view,
                &actions_ui::PanelView {
                    title: &it.title,
                    actions: &actions,
                    sel: 0,
                    filter: "",
                },
            );
        }
    }
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
        o.output,
        canvas.w,
        canvas.h,
        items.len()
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
