// PiGauge frontend — connects to WebSocket for live data,
// falls back to mock data when WS is unavailable.

const WS_PORT_OFFSET = 1;

const $rpm = document.getElementById("rpm");
const $speed = document.getElementById("speed");
const $coolant = document.getElementById("coolant");
const $throttle = document.getElementById("throttle");
const $battery = document.getElementById("battery");
const $dot = document.getElementById("status-dot");
const $mockBadge = document.getElementById("mock-badge");

let useMock = false;
let mockTimer = null;

// ---- Rendering ----

function render(snap) {
  // Feed canvas scene
  window.sceneState.rpm = snap.rpm || 0;
  window.sceneState.speed_kph = snap.speed_kph || 0;
  window.sceneState.throttle_pct = snap.throttle_pct || 0;
  window.sceneState.connected = !!snap.connected;

  // RPM + color
  $rpm.textContent = snap.rpm != null ? String(snap.rpm) : "----";
  if (snap.rpm != null && snap.rpm > 6000) {
    $rpm.style.color = "#eb564a";
  } else if (snap.rpm != null && snap.rpm > 4500) {
    $rpm.style.color = "#f5a623";
  } else {
    $rpm.style.color = "#fff";
  }

  // Speed
  if (snap.speed_kph != null) {
    $speed.textContent = String(Math.round(snap.speed_kph * 0.621371));
  } else {
    $speed.textContent = "---";
  }

  // Coolant
  if (snap.coolant_c != null) {
    $coolant.textContent = String(Math.round(snap.coolant_c * 9 / 5 + 32));
  } else {
    $coolant.textContent = "---";
  }

  // Throttle
  $throttle.textContent =
    snap.throttle_pct != null ? String(snap.throttle_pct) : "---";

  // Battery
  $battery.textContent =
    snap.battery_v != null ? snap.battery_v.toFixed(1) : "--.-";

  // Status dot
  $dot.className = "dot " + (snap.connected ? "connected" : "disconnected");
}

// ---- Mock data ----

function startMock() {
  if (useMock) return;
  useMock = true;
  $mockBadge.classList.remove("hidden");

  let t = 0;
  mockTimer = setInterval(() => {
    t += 0.04;
    const cycle = t % 30;
    let rpm, speed, throttle;

    if (cycle < 5) {
      rpm = 780 + Math.random() * 40;
      speed = 0;
      throttle = 0;
    } else if (cycle < 10) {
      const p = (cycle - 5) / 5;
      rpm = 800 + p * 4200;
      speed = p * 100;
      throttle = 40 + p * 55;
    } else if (cycle < 15) {
      rpm = 2800 + Math.sin(t * 2) * 300;
      speed = 95 + Math.sin(t) * 5;
      throttle = 20 + Math.sin(t * 1.5) * 8;
    } else if (cycle < 20) {
      const p = (cycle - 15) / 5;
      rpm = 3000 + p * 3800;
      speed = 100 + p * 60;
      throttle = 85 + p * 15;
    } else if (cycle < 25) {
      const p = (cycle - 20) / 5;
      rpm = 6800 - p * 5000;
      speed = 160 - p * 140;
      throttle = Math.max(0, 100 - p * 100);
    } else {
      const p = (cycle - 25) / 5;
      rpm = 1800 - p * 1000;
      speed = 20 - p * 20;
      throttle = 5 - p * 5;
    }

    render({
      rpm: Math.round(Math.max(750, rpm)),
      speed_kph: Math.round(Math.max(0, speed)),
      coolant_c: Math.round(88 + 5 * Math.sin(t * 0.08)),
      throttle_pct: Math.round(Math.max(0, Math.min(100, throttle))),
      battery_v: 13.6 + 0.8 * Math.sin(t * 0.03),
      // Mock is not real OBD — keep status dot "disconnected" so it is not confused with a live link.
      connected: false,
    });
  }, 33);
}

function stopMock() {
  if (!useMock) return;
  useMock = false;
  $mockBadge.classList.add("hidden");
  if (mockTimer) { clearInterval(mockTimer); mockTimer = null; }
}

// ---- WebSocket ----

function connectWs() {
  const wsPort = Number(location.port) + WS_PORT_OFFSET;
  const url = `ws://${location.hostname}:${wsPort}`;

  let ws;
  try { ws = new WebSocket(url); } catch {
    startMock();
    setTimeout(connectWs, 3000);
    return;
  }

  ws.onopen = () => { stopMock(); };
  ws.onmessage = (e) => { try { render(JSON.parse(e.data)); } catch {} };
  ws.onclose = () => { startMock(); setTimeout(connectWs, 2000); };
  ws.onerror = () => { ws.close(); };
}

// ---- Kiosk: cursor hide + hold-Escape to exit fullscreen ----
// Cursor is hidden by default (CSS cursor:none on body).
// Short Escape tap: re-enters fullscreen, cursor stays hidden.
// Escape held ≥ 1 s: stays out of fullscreen, cursor becomes visible.
// Q: close the app window.

const ESCAPE_HOLD_MS = 1000;
let _escDown = null;

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && _escDown === null) {
    _escDown = Date.now();
  }
  if (e.key === "q" || e.key === "Q") {
    e.preventDefault();
    window.close();
  }
}, true);

document.addEventListener("keyup", (e) => {
  if (e.key === "Escape" && _escDown !== null) {
    const held = Date.now() - _escDown;
    _escDown = null;
    if (held < ESCAPE_HOLD_MS) {
      // Short press — re-enter fullscreen and keep cursor hidden
      document.documentElement.requestFullscreen().catch(() => {});
    } else {
      // Long hold — show cursor, stay out of fullscreen
      document.body.classList.add("cursor-visible");
    }
  }
}, true);

// Hide cursor again whenever fullscreen is re-entered via JS API
document.addEventListener("fullscreenchange", () => {
  if (document.fullscreenElement) {
    document.body.classList.remove("cursor-visible");
  }
});

// ---- Fullscreen on load ----
// --start-fullscreen can grab the wrong compositor geometry before Wayland has
// fully negotiated the surface size.  By the time JS runs Chromium already
// thinks it's in fullscreen, so a plain requestFullscreen() is a no-op.
// Fix: exit whatever (possibly wrong-sized) fullscreen state exists, then
// re-enter — same as the manual Escape → F11 workaround that already works.
document.addEventListener("DOMContentLoaded", () => {
  setTimeout(async () => {
    try {
      if (document.fullscreenElement) {
        await document.exitFullscreen();
        // Brief pause so the compositor processes the exit before we re-enter.
        await new Promise(r => setTimeout(r, 150));
      }
      await document.documentElement.requestFullscreen();
    } catch (_) {}
  }, 500);
});

// ---- Boot ----

if (location.protocol === "file:") {
  startMock();
} else {
  connectWs();
  // Do not start mock just because OBD is disconnected — the server still sends
  // JSON with connected:false and null fields. Mock is only for missing WS (see onclose / file:).
}
