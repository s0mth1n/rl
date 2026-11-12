#![allow(dead_code)]
#[path = "../src/actions_ui.rs"]
mod actions_ui;
#[allow(dead_code)]
#[path = "../src/dates.rs"]
mod dates;
#[allow(dead_code)]
#[path = "../src/hist.rs"]
mod hist;

#[path = "../src/clip.rs"]
mod clip;
#[path = "../src/config.rs"]
mod config;
#[path = "../src/icons.rs"]
mod icons;
#[path = "../src/items.rs"]
mod items;
#[path = "../src/settings.rs"]
mod settings;
#[cfg(test)]
#[path = "../src/testutil.rs"]
mod testutil;
#[path = "../src/ui.rs"]
mod ui;

use std::hint::black_box;
use std::time::Instant;

fn report(label: &str, mut samples: Vec<f64>) {
    samples.sort_by(f64::total_cmp);
    println!(
        "{label}: median={:.3}us p95={:.3}us n={}",
        samples[samples.len() / 2],
        samples[(samples.len() * 95 / 100).min(samples.len() - 1)],
        samples.len()
    );
}

fn rss_kib() -> String {
    std::fs::read_to_string("/proc/self/status")
        .unwrap_or_default()
        .lines()
        .find(|line| line.starts_with("VmRSS:"))
        .unwrap_or("VmRSS: unavailable")
        .to_owned()
}

fn main() {
    let root = tempfile::tempdir().unwrap();
    let applications = root.path().join("applications");
    std::fs::create_dir(&applications).unwrap();
    for i in 0..600 {
        let name = [
            "Firefox Browser",
            "Code Editor",
            "Terminal",
            "Image Viewer",
            "Music Player",
            "Document Reader",
        ][i % 6];
        std::fs::write(applications.join(format!("fixture-{i:03}.desktop")), format!(
            "[Desktop Entry]\nType=Application\nName={name} {i:03}\nComment=Offline benchmark fixture\nExec=true\n"
        )).unwrap();
    }
    std::env::set_var("XDG_DATA_HOME", root.path());
    std::env::set_var("XDG_DATA_DIRS", root.path());
    let started = Instant::now();
    let cfg = config::Config::default();
    let apps = items::scan_apps(&cfg);
    assert_eq!(apps.len(), 600);
    let font = rusttype::Font::try_from_vec(
        std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf").unwrap(),
    )
    .unwrap();
    let cache = icons::Cache::new("", "");
    let history = clip::History::new(100);
    println!("no compositor; 600 desktop files; 640px; scale 1; DejaVu Sans; warm filesystem");
    println!(
        "init_scan_font_cache_us={:.3}; {}",
        started.elapsed().as_secs_f64() * 1e6,
        rss_kib()
    );
    let search = items::Search {
        cfg: &cfg,
        apps: &apps,
        files: &[],
        history: &history,
    };
    let queries = [
        "", "f", "fi", "fir", "fire", "firef", "firefox", "b", "br", "bro", "browser", "code",
        "zzz", "é",
    ];
    for query in queries {
        for _ in 0..30 {
            black_box(search.run(black_box(query)));
        }
        let mut samples = Vec::new();
        for _ in 0..100 {
            let t = Instant::now();
            black_box(search.run(black_box(query)));
            samples.push(t.elapsed().as_secs_f64() * 1e6);
        }
        report(&format!("search {query:?}"), samples);
    }
    let rows = search.run("firefox");
    let h = ui::panel_height(&rows, 8);
    let view = ui::View {
        query: "firefox",
        items: &rows,
        sel: 0,
        top: 0,
        vp: ui::viewport(&rows, 8),
        theme: cfg.theme(),
        w: 640,
        h,
        scale: 1,
        font_size: 15.0,
        icons: &cache,
    };
    let mut samples = Vec::new();
    let mut last = ui::Canvas::new(640, h);
    for i in 0..110 {
        let t = Instant::now();
        let mut cv = ui::Canvas::new(640, h);
        ui::draw(&mut cv, &font, &view);
        black_box(&cv.buf);
        if i >= 10 {
            samples.push(t.elapsed().as_secs_f64() * 1e6);
        }
        last = cv;
    }
    report("canvas_alloc_draw_no_wayland_no_icon_io", samples);
    println!("after_draw_{}", rss_kib());
    if let Some(path) = std::env::args().nth(1) {
        for px in last.buf.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
            let alpha = px[3] as u32;
            for c in &mut px[..3] {
                *c = (*c as u32 * 255).checked_div(alpha).unwrap_or(0).min(255) as u8;
            }
        }
        let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), last.w, last.h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&last.buf)
            .unwrap();
    }
}
