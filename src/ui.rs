 use std::{
     path::PathBuf,
     sync::{Arc, Mutex},
     thread,
     time::{Duration, Instant},
 };
 
use anyhow::{Context, Result};
 use log::{info, warn};
 use sdl2::{pixels::Color, rect::Rect};
 
 use crate::obd::{Elm327, ObdConfig, ObdSnapshot};
 
 pub struct UiConfig {
     pub obd_dev: PathBuf,
     pub obd_baud: u32,
     pub fps: u32,
     pub poll_interval: Duration,
 }
 
 pub fn run(cfg: UiConfig) -> Result<()> {
     let shared = Arc::new(Mutex::new(ObdSnapshot::default()));
     spawn_obd_thread(
         shared.clone(),
         ObdConfig {
             dev: cfg.obd_dev.clone(),
             baud: cfg.obd_baud,
             timeout: Duration::from_millis(120),
         },
         cfg.poll_interval,
     );
 
    let sdl = sdl2::init().map_err(anyhow::Error::msg).context("sdl init")?;
    let video = sdl.video().map_err(anyhow::Error::msg).context("sdl video")?;
    let ttf = sdl2::ttf::init().map_err(anyhow::Error::msg).context("sdl ttf")?;
 
     // A small set of fonts commonly present on Raspberry Pi OS.
     // User can symlink their preferred font to this path.
     let font_path_candidates = [
         "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
         "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
         "/usr/share/fonts/truetype/freefont/FreeSans.ttf",
     ];
     let font_path = font_path_candidates
         .iter()
         .find(|p| std::path::Path::new(p).exists())
         .map(|s| *s)
         .context("no usable TTF font found; install DejaVuSans or LiberationSans")?;
 
    let font_speed = ttf
        .load_font(font_path, 98)
        .map_err(anyhow::Error::msg)?;
    let font_label = ttf
        .load_font(font_path, 20)
        .map_err(anyhow::Error::msg)?;
    let font_value = ttf
        .load_font(font_path, 34)
        .map_err(anyhow::Error::msg)?;
    let font_unit = ttf
        .load_font(font_path, 24)
        .map_err(anyhow::Error::msg)?;
    let font_small = ttf
        .load_font(font_path, 17)
        .map_err(anyhow::Error::msg)?;
 
    let window = video
        .window("PiGauge", 480, 320)
        .position_centered()
        .fullscreen_desktop()
        .opengl()
        .build()
        .map_err(anyhow::Error::msg)
        .context("create window")?;
 
    let mut canvas = window
        .into_canvas()
        .accelerated()
        .present_vsync()
        .build()
        .map_err(anyhow::Error::msg)
        .context("create canvas")?;
    let mut fullscreen = true;
 
     let creator = canvas.texture_creator();
 
    let mut events = sdl
        .event_pump()
        .map_err(anyhow::Error::msg)
        .context("event pump")?;
 
     let fps = cfg.fps.max(1);
     let frame = Duration::from_millis((1000 / fps as u64).max(1));
     info!(
         "ui started: dev={} baud={} fps={} poll={:?}",
         cfg.obd_dev.display(),
         cfg.obd_baud,
         fps,
         cfg.poll_interval
     );
 
     'run: loop {
         let start = Instant::now();
         for e in events.poll_iter() {
             use sdl2::event::Event;
             use sdl2::keyboard::Keycode;
             match e {
                 Event::Quit { .. } => break 'run,
                 Event::KeyDown {
                     keycode: Some(Keycode::Escape),
                     ..
                } => {
                    if fullscreen {
                        canvas
                            .window_mut()
                            .set_fullscreen(sdl2::video::FullscreenType::Off)
                            .map_err(anyhow::Error::msg)?;
                        fullscreen = false;
                    }
                }
                Event::KeyDown {
                    keycode: Some(Keycode::F11),
                    ..
                } => {
                    if !fullscreen {
                        canvas
                            .window_mut()
                            .set_fullscreen(sdl2::video::FullscreenType::Desktop)
                            .map_err(anyhow::Error::msg)?;
                        fullscreen = true;
                    }
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Q),
                    ..
                } => break 'run,
                 _ => {}
             }
         }
 
         let snap = *shared.lock().unwrap_or_else(|p| p.into_inner());
 
         // Render.
         canvas.set_draw_color(Color::RGB(7, 9, 12));
         canvas.clear();
 
        let (w, h) = canvas.output_size().map_err(anyhow::Error::msg)?;
        let white = Color::RGB(235, 240, 245);
        let muted = Color::RGB(145, 158, 172);
        let accent = Color::RGB(64, 211, 162);
        let panel = Color::RGB(14, 18, 24);
        let divider = Color::RGB(24, 30, 39);
        let danger = Color::RGB(235, 86, 74);
        let pad = ((w.min(h) as i32) / 25).max(10);

        blit_text(
            &mut canvas,
            &creator,
            &font_small,
            "PIGAUGE",
            muted,
            pad,
            pad - 2,
        )?;
        blit_text(
            &mut canvas,
            &creator,
            &font_small,
            "LIVE",
            muted,
            (w as i32) - pad - 42,
            pad - 2,
        )?;

        let ok_recent = snap
            .last_ok
            .map(|t| t.elapsed() < Duration::from_secs(2))
            .unwrap_or(false);
        canvas.set_draw_color(if ok_recent { accent } else { danger });
        canvas
            .fill_rect(Rect::new((w as i32) - pad - 12, pad + 3, 8, 8))
            .map_err(anyhow::Error::msg)?;

         let speed = snap
             .speed_kph
            .map(|v| {
                let mph = (v as f32) * 0.621_371;
                format!("{:03}", mph.round() as i32)
            })
             .unwrap_or_else(|| "---".into());
         blit_text(
             &mut canvas,
             &creator,
            &font_label,
            "SPEED",
             muted,
            pad,
            pad + 22,
         )?;
        blit_text(
            &mut canvas,
            &creator,
            &font_speed,
            &speed,
            white,
            pad - 2,
            pad + 44,
        )?;
        blit_text(
            &mut canvas,
            &creator,
            &font_unit,
            "mph",
            muted,
            pad + 235,
            pad + 108,
        )?;

        let panel_y = (h as i32) - 108;
        canvas.set_draw_color(panel);
        canvas
            .fill_rect(Rect::new(pad, panel_y, w - (pad as u32 * 2), 92))
            .map_err(anyhow::Error::msg)?;
        canvas.set_draw_color(divider);
        let col_w = (w - (pad as u32 * 2)) / 3;
        for i in 1..3 {
            let x = pad + (col_w as i32 * i);
            canvas
                .fill_rect(Rect::new(x, panel_y + 10, 1, 72))
                .map_err(anyhow::Error::msg)?;
        }

         let rpm = snap
             .rpm
             .map(|v| format!("{v:>4}"))
             .unwrap_or_else(|| "----".into());
         blit_text(
             &mut canvas,
             &creator,
            &font_small,
             "RPM",
             muted,
            pad + 14,
            panel_y + 14,
         )?;
         blit_text(
             &mut canvas,
             &creator,
            &font_value,
             &rpm,
             white,
            pad + 14,
            panel_y + 34,
         )?;
 
         let coolant = snap
             .coolant_c
            .map(|c| {
                let f = (c as f32) * 9.0 / 5.0 + 32.0;
                format!("{:>3}F", f.round() as i32)
            })
            .unwrap_or_else(|| "---F".into());
         blit_text(
             &mut canvas,
             &creator,
            &font_small,
             "COOLANT",
             muted,
            pad + col_w as i32 + 12,
            panel_y + 14,
         )?;
         blit_text(
             &mut canvas,
             &creator,
            &font_value,
             &coolant,
             white,
            pad + col_w as i32 + 12,
            panel_y + 34,
         )?;
 
         let thr = snap
             .throttle_pct
             .map(|v| format!("{v:>3}%"))
             .unwrap_or_else(|| "---%".into());
         blit_text(
             &mut canvas,
             &creator,
            &font_small,
            "THR / VBAT",
             muted,
            pad + (col_w as i32 * 2) + 12,
            panel_y + 14,
         )?;
 
         let v = snap
             .battery_v
             .map(|x| format!("{x:>4.1}V"))
             .unwrap_or_else(|| "--.-V".into());
        let thr_v = format!("{thr}  {v}");
         blit_text(
             &mut canvas,
             &creator,
            &font_value,
            &thr_v,
             white,
            pad + (col_w as i32 * 2) + 12,
            panel_y + 34,
         )?;
 
         canvas.present();
 
         // Frame pacing.
         let elapsed = start.elapsed();
         if elapsed < frame {
             thread::sleep(frame - elapsed);
         }
     }
 
     Ok(())
 }
 
 fn spawn_obd_thread(shared: Arc<Mutex<ObdSnapshot>>, cfg: ObdConfig, poll: Duration) {
     thread::spawn(move || {
         let mut backoff = Duration::from_millis(250);
         loop {
            let mut elm = match Elm327::open(&cfg) {
                Ok(v) => v,
                Err(e) => {
                    warn!("obd open error: {e:#}");
                    std::thread::sleep(backoff);
                    backoff = (backoff * 2).min(Duration::from_secs(10));
                    continue;
                }
            };

            if let Err(e) = elm.init_auto() {
                warn!("obd init error: {e:#}");
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_secs(10));
                continue;
            }

            info!("obd connected");
            backoff = Duration::from_millis(250);

            loop {
                match elm.read_snapshot() {
                    Ok(s) => {
                        if let Ok(mut g) = shared.lock() {
                            let prev_ok = g.last_ok;
                            *g = s;
                            if g.last_ok.is_none() {
                                g.last_ok = prev_ok;
                            }
                        }
                        thread::sleep(poll);
                    }
                    Err(e) => {
                        warn!("obd read error: {e:#}");
                        if let Ok(mut g) = shared.lock() {
                            g.rpm = None;
                            g.speed_kph = None;
                            g.coolant_c = None;
                            g.throttle_pct = None;
                            g.battery_v = None;
                        }
                        std::thread::sleep(backoff);
                        backoff = (backoff * 2).min(Duration::from_secs(10));
                        break;
                    }
                }
            }
         }
     });
 }
 
 fn blit_text(
     canvas: &mut sdl2::render::Canvas<sdl2::video::Window>,
     creator: &sdl2::render::TextureCreator<sdl2::video::WindowContext>,
     font: &sdl2::ttf::Font,
     text: &str,
     color: Color,
     x: i32,
     y: i32,
 ) -> Result<()> {
     let surface = font
         .render(text)
         .blended(color)
        .map_err(anyhow::Error::msg)
        .context("render text")?;
     let texture = creator
         .create_texture_from_surface(&surface)
        .map_err(anyhow::Error::msg)
        .context("text texture")?;
     let target = Rect::new(x, y, surface.width(), surface.height());
    canvas
        .copy(&texture, None, Some(target))
        .map_err(anyhow::Error::msg)?;
     Ok(())
 }
 
