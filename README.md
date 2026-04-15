# PiGauge

A minimal, fast OBD2 dashboard for a Raspberry Pi 5 driving a small in-car display, plus a power-loss watcher that shuts the Pi down cleanly when the car's 12V rail drops. The UI is a web app (HTML/CSS/JS) served locally by a Rust backend over HTTP + WebSocket. On the Pi, Chromium runs in kiosk mode pointed at `localhost`.

This README focuses on *architecture and the reasoning behind it*. For command-line flags and flags-only reference, see `src/main.rs`.

---

## What PiGauge is

PiGauge is a purpose-built, always-on car dashboard. When the car starts, the Pi boots, the Rust backend comes up under systemd, Chromium opens fullscreen on `http://localhost:8080`, and the driver sees live gauges (RPM, speed, coolant temp, throttle, battery voltage). When power is cut, a separate process watches a GPIO line and performs a clean shutdown so the SD card / eMMC doesn't corrupt.

Two binaries-in-one (via `clap` subcommands):

- `pigauge ui` — OBD polling + HTTP + WebSocket + serves the `web/` frontend
- `pigauge power-watch` — GPIO polling + debounce + shutdown execution

They run as independent systemd services so a crash in one never takes down the other.

---

## Architecture

```
                 ┌───────────────────────────────────────────────┐
                 │                  Raspberry Pi 5               │
                 │                                               │
    ELM327 ──USB─┤► /dev/ttyUSB0                                 │
    (OBD2)       │       │                                       │
                 │       ▼                                       │
                 │  ┌─────────────┐   Arc<Mutex<ObdSnapshot>>    │
                 │  │ obd thread  │──────────────┐                │
                 │  │ (src/obd.rs)│              │                │
                 │  └─────────────┘              ▼                │
                 │                       ┌─────────────┐          │
                 │                       │ ws thread   │──────┐   │
                 │                       │ (src/ui.rs) │      │   │
                 │                       └─────────────┘      │   │
                 │                       ┌─────────────┐      │   │
                 │                       │ http thread │      │   │
                 │                       │ (src/ui.rs) │──┐   │   │
                 │                       └─────────────┘  │   │   │
                 │                                        ▼   ▼   │
                 │                              ┌──────────────┐  │
    5" Display ──┤────────────────────────◄─────┤ Chromium     │  │
    (HDMI)       │    localhost:8080 / :8081    │ (kiosk mode) │  │
                 │                              └──────────────┘  │
                 │                                                │
    Car 12V ─────┤►  X1200 UPS HAT ──► Pi 5V rail                 │
                 │         │                                       │
                 │         └─► PLD_PIN (GPIO6, 3.3V) ──► power-watch ──►shutdown
                 │                                 (src/power.rs) │
                 └───────────────────────────────────────────────┘
```

### Data flow (UI mode)

1. **OBD thread** (`src/obd.rs`, spawned from `ui::run`) opens the serial port, runs the ELM327 init sequence (`ATZ`, `ATE0`, `ATL0`, `ATS0`, `ATH0`, `ATSP0`, `ATAT1`), then polls a small set of Mode 01 PIDs plus `ATRV` for battery voltage on a fixed interval (default 200 ms). Each successful read updates a shared `Arc<Mutex<ObdSnapshot>>`. Transient read errors trigger exponential backoff (250 ms → 10 s) and the snapshot's `connected` flag goes false.
2. **WebSocket thread** accepts connections on `port + 1` and, per connection, spawns a thread that reads the current snapshot at `1000/fps` ms (default 30 FPS → ~33 ms), serializes it to JSON via `serde`, and sends it as a text frame. If the last successful read is >2s stale, `connected` is forced false before sending — so the UI can't show live-looking data from a dead link.
3. **HTTP thread** uses `tiny_http` to serve static files from `web/` with hand-rolled MIME detection. It's deliberately dumb: no templating, no caching headers, no compression. The Pi is serving one client on the loopback interface.
4. **Frontend** (`web/app.js`) opens a WebSocket to `ws://<host>:<port+1>`. On each message it updates DOM text nodes for the gauges and pushes values into `window.sceneState` so `scene.js` can drive a canvas-based pixel-art driving scene.
5. **Mock fallback**: if the frontend is opened from `file://` (local dev) or the WebSocket closes, `app.js` starts a 30 FPS mock loop that synthesizes a 30-second drive cycle. This is what makes "double-click `web/index.html`" a full dev loop on a Mac with no hardware and no Rust build.

### Data flow (PowerWatch mode)

`src/power.rs` opens a GPIO line via `gpio-cdev` (character device API, works on Bookworm/Pi 5), polls it at 10 Hz, and watches for a sustained LOW state. Convention: **HIGH = external car power present**. The state machine is intentionally boring:

- `high` → clear the low-timer
- `low` → start (or keep) the low-timer
- low held ≥ `debounce_ms` → wait `shutdown_delay_ms`, then exec `/sbin/shutdown -h now`

The debounce exists because starter motors and big inductive loads produce brief 12V sags — you don't want a momentary dip to shut the Pi down at a red light. The post-confirm delay gives the filesystem a moment to flush.

---

## Tech stack

### Backend — Rust

| Crate          | Purpose                                                                  |
| -------------- | ------------------------------------------------------------------------ |
| `clap`         | CLI parsing + env-var binding (`PIGAUGE_*`). Subcommands: `ui`, `power-watch`. |
| `serialport`   | Cross-platform serial I/O for the ELM327 USB adapter.                    |
| `gpio-cdev`    | GPIO via `/dev/gpiochipN` (chardev API). Required for Pi 5 / Bookworm.    |
| `tiny_http`    | Tiny blocking HTTP server. ~300 LOC of code in `ui.rs`, no framework.    |
| `tungstenite`  | Bare WebSocket server. No `tokio`, no async — one thread per client.     |
| `serde` / `serde_json` | Serialize `ObdSnapshot` to JSON for the WS frames.               |
| `anyhow`       | Ergonomic error context up the stack.                                    |
| `env_logger` + `log` | `RUST_LOG=info` to see obd init, ws connect, power state flips.   |

No Tokio, no Axum, no Actix. The whole thing is synchronous threaded code, which matches the problem: one serial port, one WS client, one HTTP client. Total dependency tree is small, build times on the Pi 5 are acceptable, and the binary is a single `target/release/pigauge`.

### Frontend — plain HTML / CSS / JS

No framework, no bundler, no build step. `web/index.html` pulls `style.css`, `scene.js`, and `app.js` directly. The entire frontend is:

- `index.html` — markup for the gauge readouts and a `<canvas id="scene">`
- `style.css` — gauge layout, colors, typography, the mock badge
- `app.js` — WebSocket client, DOM updates, mock generator, kiosk `q`-to-quit
- `scene.js` — canvas rendering for the pixel-art driving scene (car + backgrounds), driven by `window.sceneState`
- `car_facing_right/`, `background_layers/` — pixel-art PNG assets

The backend serves this over HTTP, but you can also just open `web/index.html` from the filesystem on a Mac — `app.js` detects `location.protocol === "file:"` and runs the mock loop instead of trying to connect.

### Runtime — systemd on Pi OS

Three units under `systemd/`:

- `pigauge-ui.service` — system unit, starts the Rust backend as root (needs serial access).
- `pigauge-powerwatch.service` — system unit, starts the GPIO watcher.
- `pigauge-kiosk.service` — **user** unit, runs Chromium in `--app --start-fullscreen` mode pointed at `http://localhost:8080`.

Splitting kiosk into a user unit means X/Wayland session handling lives with the user, and the backend stays independent. If Chromium crashes, systemd restarts just the kiosk; the OBD polling never misses a beat.

### Hardware

- Raspberry Pi 5
- 5-inch HDMI display (~480×320, portrait-capable)
- ELM327 USB OBD2 adapter (common, ~$15)
- Geekworm X1200 UPS HAT — carries 2× 18650 cells, keeps the Pi alive well beyond the 15 s shutdown window after car power is cut. Exposes a `PLD_PIN` on **GPIO6** that is HIGH while USB-C input is present and LOW when input is lost. The pin is already at 3.3 V logic, so no level shifter is needed.
- MAX17040G+ fuel gauge on the X1200 at I2C address `0x36` (bus 1) — not currently read by the backend, but available for a future battery-percentage gauge.

---

## Key tech choices — the *why*

### Why a web UI instead of SDL2 / native

The original version used SDL2 for direct framebuffer rendering. It was pivoted to a browser UI because:

- **Animation and styling are free in CSS/JS.** Keyframes, transforms, filters, gradients, variable fonts — all trivial. In SDL2 every one of those is hand-rolled pixel math.
- **Video playback is trivial.** `<video>` elements are a one-liner; decoding H.264 in SDL2 is a project.
- **Hot-reload dev loop.** Edit `style.css`, hit refresh in Chrome on the Mac, see it. No Rust recompile, no Pi deploy, no hardware in the loop.
- **Asset tooling.** PNGs, SVGs, web fonts, devtools — everything the web platform gives you for free.

### Why Rust in the backend at all

You could write the OBD poller in Python. Rust earns its place because:

- **Serial + threads + binary deploy with zero runtime.** One static binary, systemd, done. No venv, no `pip install` on a car.
- **Determinism and memory.** A dashboard that leaks memory is a dashboard that dies mid-drive.
- **Shared ownership of the snapshot.** `Arc<Mutex<ObdSnapshot>>` between the OBD thread and the per-client WS thread is type-checked correct.
- **`gpio-cdev`** is first-class and works cleanly on Pi 5's `/dev/gpiochip4`.

### Why no async runtime

Tokio would be overkill. Exactly one OBD device, one WS client (Chromium on the same box), one HTTP client. Blocking threads are simpler to reason about and the binary stays small.

### Why two separate processes (UI vs PowerWatch)

The power-loss watcher is a **safety-critical** loop and must never be blocked by a serial hang, a WS client misbehaving, or a Chromium restart. Running it in its own systemd unit means:

- Independent restart policy
- Independent privilege (power-watch runs as root for `shutdown`; UI could drop privs later)
- A panic in UI code cannot prevent a clean shutdown when the car loses power

### Why mock data in the browser itself

Putting the mock generator in `app.js` (not the Rust side) means the dev loop on a Mac requires *nothing* — no Rust build, no backend running. Double-click `web/index.html`, iterate. This is a specific optimization for "vibe-coding" on a laptop where the car and the ELM327 and the Pi are nowhere nearby.

---

## Advantages

- **Two-language sweet spot.** Rust where correctness and I/O matter; browser where visual iteration matters. Each side plays to its strength.
- **Fast local dev loop.** `web/index.html` with mock data gives sub-second iteration on the whole UI. No hardware, no Rust build, no Pi involved.
- **Tiny runtime surface.** A single Rust binary plus a `web/` folder of static assets. Whole thing could be scp'd to another Pi and run.
- **No frontend build step.** No bundler, no `node_modules`, no framework churn. HTML, CSS, JS, PNGs.
- **Clean isolation of safety code.** PowerWatch is ~80 lines and has no dependency on the UI subsystem.
- **Crash isolation via systemd.** UI, PowerWatch, and Kiosk are independent units.
- **Hardware-independent frontend.** The UI is just a WebSocket client. Swap the car for a simulator, swap the Pi for a laptop, nothing in the frontend changes.
- **Networkless-by-default.** Everything runs on `localhost`. No wifi in the car, no captive-portal headaches, no surprise updates while driving.

## Disadvantages / tradeoffs

- **Chromium is heavy.** A whole browser engine to render a few gauges is the single biggest runtime cost on the Pi. It works, but it's not what you'd call minimalist.
- **Boot time.** Pi boot + systemd + Chromium startup is ~10–15 s until the user sees gauges. A native framebuffer renderer would be faster to first-paint.
- **Serial-only OBD, polling-based.** Each Mode 01 PID is a round-trip over a slow ELM327. Polling RPM + speed + coolant + throttle + voltage at 200 ms is already most of what the adapter can do. For richer data you'd want to move to an OBD interface that supports CAN passive sniffing (ELM327 doesn't, really).
- **`tiny_http` is blocking and single-threaded per request.** Fine for one kiosk client, bad if you ever wanted multiple viewers.
- **No TLS, no auth.** The WS server binds `0.0.0.0`. On a private in-car network that's fine; anywhere else it isn't.
- **Two snapshot freshness paths.** OBD thread stamps `last_ok`; WS thread re-checks staleness before send. It works but there are now two places that know the "2 second stale" rule — easy to drift.
- **Mock data lives in the UI layer.** Great for iteration velocity, but it means the frontend has a fake-data codepath shipped into production. A misfiring `onclose` could briefly flash mock data at the driver. (Currently mitigated by never setting `connected: true` in mock mode.)
- **Power-watch debounce is pure polling.** A GPIO edge interrupt would be more responsive and lower-CPU than 10 Hz polling, though at this rate it's negligible.
- **No persistence.** Nothing is logged to disk — no trip history, no fault codes stored, no replay.

---

## Current OBD coverage

| Metric         | PID / Cmd | Unit      | Parser             |
| -------------- | --------- | --------- | ------------------ |
| Engine RPM     | `010C`    | rpm       | `((A*256)+B)/4`    |
| Vehicle speed  | `010D`    | km/h      | `A`                |
| Coolant temp   | `0105`    | °C        | `A - 40`           |
| Throttle pos   | `0111`    | %         | `A * 100 / 255`    |
| Battery volt   | `ATRV`    | V         | ELM327 direct      |

The frontend converts km/h → mph and °C → °F for display.

---

## Future plans

### More OBD data
- **Intake air temp** (`010F`), **MAF rate** (`0110`), **engine load** (`0104`), **fuel trim** STFT/LTFT (`0106`/`0107`)
- **Intake manifold pressure** (`010B`) — boost gauge for the WRX
- **Calculated boost**: `MAP − baroP` or derived from MAP + engine displacement
- **Timing advance** (`010E`)
- **Fuel level** (`012F`) and **distance with MIL on** (`0121`)
- **DTC read/clear** (Mode 03 / Mode 04) — a "check engine" screen with plain-English code lookups

### Richer telemetry
- **Trip logging**: write a rolling ring buffer of snapshots to disk so you can replay the last drive
- **Session summary screen**: peak RPM, peak boost, 0→60 attempts, average MPG
- **G-sensor / IMU** via the Pi's I2C — lateral/longitudinal Gs alongside OBD data
- **GPS** via a cheap USB puck — speed cross-check, route logging, optional speedometer source

### UI
- Multiple gauge "themes" (pixel-art scene, minimalist, analog-dial, data-heavy)
- Night mode with reduced brightness and red-shifted palette
- Shift-light animation tied to RPM thresholds per-gear (requires gear inference from RPM/speed ratio)
- Warning banners on coolant overheat / low voltage / DTC present
- Short video / boot splash playback (easy now that the UI is a browser)

### Backend
- **CAN passive sniff** as an alternative to ELM327 polling, using an MCP2515-based SPI HAT. Would eliminate the round-trip-per-PID ceiling and enable reading manufacturer-specific IDs that ELM327 doesn't.
- **UDS / Mode 22** support for live data that vendors hide behind extended diagnostics
- **Structured logging** to a rotating file so you can post-mortem a misbehaving adapter
- **Edge-triggered GPIO** in `power.rs` via `gpio-cdev` events instead of 10 Hz polling
- **A second WebSocket endpoint** for configuration / live tuning of poll rate + PID set without restarting

### Integrations
- Push trip summaries to a companion phone app over Bluetooth when the car parks
- Optional uplink (when on home wifi) to sync trip logs to a home server

---

## Repo layout

```
PiGauge/
├── Cargo.toml              # Rust dependencies (small, intentional)
├── src/
│   ├── main.rs             # clap CLI: `ui` and `power-watch` subcommands
│   ├── obd.rs              # ELM327 driver, PID parsers, ObdSnapshot
│   ├── ui.rs               # http + ws servers, obd polling thread
│   └── power.rs            # GPIO poll loop + shutdown exec
├── web/
│   ├── index.html          # gauge markup
│   ├── style.css           # gauge styling
│   ├── app.js              # WS client + mock fallback + render
│   ├── scene.js            # canvas driving scene
│   ├── background_layers/  # parallax PNGs
│   └── car_facing_right/   # car sprite states
├── systemd/
│   ├── pigauge-ui.service
│   ├── pigauge-powerwatch.service
│   └── pigauge-kiosk.service
└── README.md               # this file
```

---

## Running it

### On a Mac (UI iteration, no hardware)

Open `web/index.html` in a browser. Mock data auto-activates; edit CSS/JS and refresh.

### On a Pi with an OBD adapter

```bash
cargo build --release

# UI (serves http://localhost:8080, ws on :8081)
./target/release/pigauge ui --obd-dev /dev/ttyUSB0

# Power watch (in a second terminal, as root)
# GPIO6 is the X1200 UPS HAT's PLD_PIN. 15 s debounce = user must actually
# be parked/shut-off (not a starter sag) before we commit to shutdown.
sudo ./target/release/pigauge power-watch --gpio-line 6 --debounce-ms 15000 --shutdown-delay-ms 500
```

Flags: `--obd-baud` (default 38400), `--fps` (default 30), `--poll-ms` (default 200), `--port` (default 8080), `--web-dir` (default `web`). All flags are also env vars: `PIGAUGE_OBD_DEV`, `PIGAUGE_OBD_BAUD`, `PIGAUGE_FPS`, `PIGAUGE_POLL_MS`, `PIGAUGE_PORT`, `PIGAUGE_WEB_DIR`.

### As systemd services

```bash
sudo cp systemd/pigauge-ui.service /etc/systemd/system/
sudo cp systemd/pigauge-powerwatch.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now pigauge-ui.service pigauge-powerwatch.service

mkdir -p ~/.config/systemd/user
cp systemd/pigauge-kiosk.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now pigauge-kiosk.service
```
