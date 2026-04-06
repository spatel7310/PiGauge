# PiGauge

Minimal OBD2 dashboard for Raspberry Pi (5" display) + a **power-loss watcher** for clean shutdown when your car power cuts (with a UPS HAT keeping the Pi alive briefly).

## What you get

- **UI mode**: web-based dashboard served locally (HTML/CSS/JS + Rust backend over WebSocket).
- **OBD2**: talks to common ELM327-style USB OBD adapters over serial.
- **PowerWatch mode**: watches a GPIO input (power-good signal) and runs `shutdown -h now` after debounce/delay.

## Build prerequisites (Raspberry Pi OS)

```bash
sudo apt update
sudo apt install -y build-essential pkg-config chromium-browser

# Rust toolchain (if you don't have it)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

## Build

```bash
cd PiGauge
cargo build --release
```

## Run UI

Plug in your USB OBD adapter, find the device name, then run:

```bash
ls /dev/ttyUSB* /dev/ttyACM* 2>/dev/null

# Example — starts HTTP on :8080, WebSocket on :8081
./target/release/pigauge ui --obd-dev /dev/ttyUSB0
```

Then open `http://localhost:8080` in a browser (or Chromium kiosk on the Pi).

Tunables:

- `--obd-baud 38400` (try 38400 first; some adapters use 115200)
- `--fps 30` (WebSocket broadcast rate)
- `--poll-ms 200`
- `--port 8080` (HTTP port; WS is port+1)
- `--web-dir web` (path to the web frontend directory)

Env vars: `PIGAUGE_OBD_DEV`, `PIGAUGE_OBD_BAUD`, `PIGAUGE_FPS`, `PIGAUGE_POLL_MS`, `PIGAUGE_PORT`, `PIGAUGE_WEB_DIR`

## Local development (mock data, no hardware needed)

Just open `web/index.html` directly in your browser — it auto-detects that the WebSocket backend isn't running and generates mock gauge data so you can iterate on the UI.

## Run PowerWatch (safe shutdown)

**Goal**: feed a GPIO input that is **HIGH when car power is present**, and goes **LOW when the cigarette-lighter power is cut**. When it stays LOW for `debounce_ms`, PiGauge waits `shutdown_delay_ms` and runs `shutdown`.

```bash
sudo ./target/release/pigauge power-watch --gpio-line 17
```

Notes:

- This uses `gpio-cdev` (character device API) for GPIO access, which works on all modern Pi OS versions including Bookworm.
- Pi 5 uses `/dev/gpiochip4` (the default). Older Pis use `/dev/gpiochip0` — pass `--gpio-chip /dev/gpiochip0` if needed.
- You **must not** feed 5V directly into a Pi GPIO. Use a divider/level shifter.

Tunables:

- `--debounce-ms 1500`
- `--shutdown-delay-ms 1500`
- `--shutdown-cmd "/sbin/shutdown -h now"`

## Systemd setup (Pi)

```bash
# 1. Backend (OBD polling + web server)
sudo cp systemd/pigauge-ui.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now pigauge-ui.service

# 2. Kiosk (fullscreen Chromium pointing at localhost)
mkdir -p ~/.config/systemd/user
cp systemd/pigauge-kiosk.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now pigauge-kiosk.service

# 3. Power watch (safe shutdown)
sudo cp systemd/pigauge-powerwatch.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now pigauge-powerwatch.service
```
