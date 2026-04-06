 # PiGauge
 
 Fast, minimal **native** OBD2 dashboard for Raspberry Pi (mini display) + a **power-loss watcher** for clean shutdown when your car power cuts (with a UPS HAT keeping the Pi alive briefly).
 
 ## What you get
 
 - **UI mode**: fullscreen SDL2 app rendering text-only gauges (low CPU/GPU).
 - **OBD2**: talks to common ELM327-style USB OBD adapters over serial.
 - **PowerWatch mode**: watches a GPIO input (power-good signal) and runs `shutdown -h now` after debounce/delay.
 
 ## Build prerequisites (Raspberry Pi OS)
 
 ```bash
 sudo apt update
 sudo apt install -y build-essential pkg-config \
   libsdl2-dev libsdl2-ttf-dev \
   fontconfig fonts-dejavu-core
 
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
 
 # Example:
 ./target/release/pigauge ui --obd-dev /dev/ttyUSB0
 ```
 
 Tunables:
 
 - `--obd-baud 38400` (try 38400 first; some adapters use 115200)
 - `--fps 30`
 - `--poll-ms 200`
 
 You can also use env vars:
 
 - `PIGAUGE_OBD_DEV=/dev/ttyUSB0`
 - `PIGAUGE_OBD_BAUD=38400`
 - `PIGAUGE_FPS=30`
 - `PIGAUGE_POLL_MS=200`

UI controls:

- `Esc`: leave fullscreen mode
- `F11`: re-enter fullscreen mode
- `Q`: quit app
 
 ## Run PowerWatch (safe shutdown)
 
 **Goal**: feed a GPIO input that is **HIGH when car power is present**, and goes **LOW when the cigarette-lighter power is cut**. When it stays LOW for `debounce_ms`, PiGauge waits `shutdown_delay_ms` and runs `shutdown`.
 
 ```bash
 sudo ./target/release/pigauge power-watch --gpio-line 17
 ```
 
 Notes:
 
 - This uses `/sys/class/gpio` for now (works on many Pi OS installs; sysfs GPIO is deprecated upstream).
 - The `--gpio-line` is the GPIO number used by sysfs (often the BCM number).
 - You **must not** feed 5V directly into a Pi GPIO. Use a divider/level shifter.
 
 Tunables:
 
 - `--debounce-ms 1500`
 - `--shutdown-delay-ms 1500`
 - `--shutdown-cmd "/sbin/shutdown -h now"`
 
## User graphical startup (recommended)

Use a user service so the UI launches inside your desktop session (Wayland), which allows reliable fullscreen behavior.

```bash
mkdir -p ~/.config/systemd/user
cp /home/spatel7310/PiGauge/systemd/pigauge-ui.service ~/.config/systemd/user/pigauge-ui.service
systemctl --user daemon-reload
systemctl --user enable pigauge-ui.service
systemctl --user start pigauge-ui.service
systemctl --user status pigauge-ui.service --no-pager
```

If you previously enabled a system-level unit, disable it:

```bash
sudo systemctl disable --now pigauge-ui.service
```
 
# PiGauge