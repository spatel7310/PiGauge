use std::{
    net::TcpListener,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use anyhow::{Context, Result};
use log::{info, warn};
use tungstenite::accept;

use crate::obd::{Elm327, ObdConfig, ObdSnapshot};

pub struct UiConfig {
    pub obd_dev: PathBuf,
    pub obd_baud: u32,
    pub fps: u32,
    pub poll_interval: Duration,
    pub port: u16,
    pub web_dir: PathBuf,
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

    let ws_shared = shared.clone();
    let ws_interval = Duration::from_millis((1000 / cfg.fps.max(1) as u64).max(16));
    let ws_addr = format!("0.0.0.0:{}", cfg.port + 1);
    thread::spawn(move || run_ws_server(&ws_addr, ws_shared, ws_interval));

    info!(
        "serving web UI on http://0.0.0.0:{} (ws on port {})",
        cfg.port,
        cfg.port + 1
    );
    run_http_server(cfg.port, &cfg.web_dir)
}

fn run_http_server(port: u16, web_dir: &PathBuf) -> Result<()> {
    let server = tiny_http::Server::http(format!("0.0.0.0:{port}"))
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("start http server")?;

    for request in server.incoming_requests() {
        let url = request.url().to_string();
        let path = if url == "/" { "/index.html" } else { &url };

        let file_path = web_dir.join(path.trim_start_matches('/'));

        match std::fs::read(&file_path) {
            Ok(data) => {
                let mime = match file_path.extension().and_then(|e| e.to_str()) {
                    Some("html") => "text/html; charset=utf-8",
                    Some("css") => "text/css; charset=utf-8",
                    Some("js") => "application/javascript; charset=utf-8",
                    Some("json") => "application/json",
                    Some("png") => "image/png",
                    Some("svg") => "image/svg+xml",
                    Some("ico") => "image/x-icon",
                    Some("mp4") => "video/mp4",
                    Some("webm") => "video/webm",
                    _ => "application/octet-stream",
                };
                let header =
                    tiny_http::Header::from_bytes("Content-Type", mime).unwrap();
                let response = tiny_http::Response::from_data(data).with_header(header);
                let _ = request.respond(response);
            }
            Err(_) => {
                let response = tiny_http::Response::from_string("404 Not Found")
                    .with_status_code(404);
                let _ = request.respond(response);
            }
        }
    }
    Ok(())
}

fn run_ws_server(addr: &str, shared: Arc<Mutex<ObdSnapshot>>, interval: Duration) {
    let listener = match TcpListener::bind(addr) {
        Ok(l) => l,
        Err(e) => {
            warn!("ws bind failed: {e}");
            return;
        }
    };
    info!("ws listening on {addr}");

    for stream in listener.incoming().flatten() {
        let shared = shared.clone();
        thread::spawn(move || {
            let mut ws = match accept(stream) {
                Ok(ws) => ws,
                Err(e) => {
                    warn!("ws accept error: {e}");
                    return;
                }
            };
            info!("ws client connected");

            loop {
                let snap = {
                    let g = shared.lock().unwrap_or_else(|p| p.into_inner());
                    let mut s = *g;
                    // Mark as disconnected if data is stale.
                    if let Some(t) = s.last_ok {
                        if t.elapsed() > Duration::from_secs(2) {
                            s.connected = false;
                        }
                    }
                    s
                };

                let json = match serde_json::to_string(&snap) {
                    Ok(j) => j,
                    Err(_) => break,
                };

                if ws.send(tungstenite::Message::Text(json)).is_err() {
                    break;
                }

                thread::sleep(interval);
            }
            info!("ws client disconnected");
        });
    }
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
                        log::debug!(
                            "obd: rpm={:?} spd={:?} clt={:?} thr={:?} bat={:?}",
                            s.rpm, s.speed_kph, s.coolant_c, s.throttle_pct, s.battery_v
                        );
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
                            g.connected = false;
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
