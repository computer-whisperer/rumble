//! Headless screenshot of a single rumble-next paradigm.
//!
//! Usage (from the workspace root):
//!
//!     cargo run -p rumble-next --example screenshot -- \
//!         --paradigm modern --out /tmp/rumble_next_modern.png
//!
//! Flags:
//!   --paradigm {modern|mumble|luna}   default: modern
//!   --out      <path>                  default: /tmp/rumble_next_<paradigm>.png
//!   --size     WxH                     default: 1280x820

use eframe::egui;
use egui_kittest::Harness;
use rumble_next::{App, Paradigm};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = parse_args(&args)?;

    // Sandbox the run so screenshots don't leak the developer's real
    // identity / saved servers / accepted certs. Each invocation gets
    // a fresh tempdir; nothing in here outlives the example. Honour an
    // existing `RUMBLE_NEXT_CONFIG_DIR` so callers that want a curated
    // settings file (e.g. screenshotting the populated recent-servers
    // list) can opt out of the sandbox.
    if std::env::var_os("RUMBLE_NEXT_CONFIG_DIR").is_none() {
        let sandbox = std::env::temp_dir().join(format!(
            "rumble-next-screenshot-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&sandbox).map_err(|e| format!("sandbox: {e}"))?;
        // SAFETY: examples are single-threaded entry points so the
        // env-var mutation happens before any thread that might read it.
        unsafe {
            std::env::set_var("RUMBLE_NEXT_CONFIG_DIR", &sandbox);
        }
    }

    let initial_paradigm = opts.paradigm;
    let dark = opts.dark;
    let mut harness = Harness::builder()
        .with_size(egui::Vec2::new(opts.width, opts.height))
        .with_pixels_per_point(2.0)
        .wgpu()
        .build_eframe(move |cc| {
            let mut app = App::new(cc).expect("App::new failed");
            app.paradigm = initial_paradigm;
            app.dark = dark;
            app
        });

    // Several steps let the ScrollArea settle + fonts tessellate.
    // Extra steps give the backend task time to establish a connection
    // when `RUMBLE_NEXT_AUTOCONNECT=1` was set.
    let autoconnecting = std::env::var("RUMBLE_NEXT_AUTOCONNECT").is_ok();
    let steps = if autoconnecting { 120 } else { 6 };
    for i in 0..steps {
        harness.step();
        if autoconnecting {
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let _ = i;
    }

    let image = harness.render()?;
    image.save(&opts.out).map_err(|e| format!("save {}: {e}", opts.out))?;
    println!("wrote {}", opts.out);
    Ok(())
}

struct Opts {
    paradigm: Paradigm,
    dark: bool,
    out: String,
    width: f32,
    height: f32,
}

fn parse_args(args: &[String]) -> Result<Opts, String> {
    let mut name = "modern".to_string();
    let mut out: Option<String> = None;
    let mut size = (1280.0_f32, 820.0_f32);
    let mut dark = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--paradigm" => {
                name = args.get(i + 1).ok_or("missing value for --paradigm")?.clone();
                i += 2;
            }
            "--dark" => {
                dark = true;
                i += 1;
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

    let paradigm = match name.as_str() {
        "modern" => Paradigm::Modern,
        "mumble" | "mumble-classic" => Paradigm::MumbleClassic,
        "luna" | "xp" => Paradigm::Luna,
        other => {
            return Err(format!("unknown paradigm '{other}' — want modern|mumble|luna"));
        }
    };

    let suffix = if dark { "_dark" } else { "" };
    let out = out.unwrap_or_else(|| format!("/tmp/rumble_next_{}{suffix}.png", name.replace('-', "_")));
    Ok(Opts {
        paradigm,
        dark,
        out,
        width: size.0,
        height: size.1,
    })
}
