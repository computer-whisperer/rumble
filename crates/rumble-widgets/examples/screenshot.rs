//! Headless screenshot of the widgets gallery.
//!
//! Usage (from the workspace root):
//!
//!     cargo run -p rumble-widgets --example screenshot -- \
//!         --theme mumble --out /tmp/gallery_mumble.png
//!
//! Flags:
//!   --theme {modern|luna|luna-dark|mumble|mumble-dark}   default: mumble
//!   --out   <path>                                       default: /tmp/gallery_<theme>.png
//!   --size  WxH                                          default: 1100x2400
//!
//! Renders via egui_kittest's wgpu backend — no window needed.

use eframe::egui;
use egui_kittest::Harness;
use rumble_widgets::gallery::Gallery;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = parse_args(&args)?;

    // Gallery::default() reads GALLERY_THEME at construction time. Set it
    // here so the Harness-constructed Gallery picks the intended theme.
    // SAFETY: single-threaded main before any threads are spawned.
    unsafe {
        std::env::set_var("GALLERY_THEME", opts.theme_index.to_string());
    }

    // 2x pixels_per_point so 1-px tree-line dots and text hairlines
    // survive the screenshot → PNG → display pipeline without being
    // culled. Doubles the PNG dimensions; layout is unchanged.
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(opts.width, opts.height))
        .with_pixels_per_point(2.0)
        .wgpu()
        .build_eframe(|_cc| Gallery::default());

    // A few steps let the ScrollArea settle, fonts tessellate, and the
    // animated LevelMeter tick through some frames.
    for _ in 0..6 {
        harness.step();
    }

    let image = harness.render()?;
    image.save(&opts.out).map_err(|e| format!("save {}: {e}", opts.out))?;
    println!("wrote {}", opts.out);
    Ok(())
}

struct Opts {
    theme_index: usize,
    out: String,
    width: f32,
    height: f32,
}

fn parse_args(args: &[String]) -> Result<Opts, String> {
    let mut theme = "mumble".to_string();
    let mut out: Option<String> = None;
    let mut size = (1100.0_f32, 2400.0_f32);

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--theme" => {
                theme = args.get(i + 1).ok_or("missing value for --theme")?.clone();
                i += 2;
            }
            "--out" => {
                out = Some(args.get(i + 1).ok_or("missing value for --out")?.clone());
                i += 2;
            }
            "--size" => {
                let v = args.get(i + 1).ok_or("missing value for --size")?;
                let (w, h) = v.split_once('x').ok_or_else(|| format!("bad --size {v} (want WxH)"))?;
                size = (
                    w.parse().map_err(|_| format!("bad width in {v}"))?,
                    h.parse().map_err(|_| format!("bad height in {v}"))?,
                );
                i += 2;
            }
            other => return Err(format!("unknown arg {other}")),
        }
    }

    let theme_index = match theme.as_str() {
        "modern" => 0,
        "luna" => 1,
        "luna-dark" => 2,
        "mumble" => 3,
        "mumble-dark" => 4,
        other => {
            return Err(format!(
                "unknown theme '{other}' — want modern|luna|luna-dark|mumble|mumble-dark",
            ));
        }
    };

    let out = out.unwrap_or_else(|| format!("/tmp/gallery_{}.png", theme.replace('-', "_")));
    Ok(Opts {
        theme_index,
        out,
        width: size.0,
        height: size.1,
    })
}
