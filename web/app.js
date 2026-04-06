// PiGauge frontend — connects to WebSocket for live data,
// falls back to mock data when WS is unavailable.

const WS_PORT_OFFSET = 1;

const $speed = document.getElementById("speed");
const $rpm = document.getElementById("rpm");
const $coolant = document.getElementById("coolant");
const $throttle = document.getElementById("throttle");
const $battery = document.getElementById("battery");
const $dot = document.getElementById("status-dot");
const $mockBadge = document.getElementById("mock-badge");

let useMock = false;
let mockTimer = null;

// ---- Rendering ----

function render(snap) {
  // Speed: kph → mph
  if (snap.speed_kph != null) {
    const mph = Math.round(snap.speed_kph * 0.621371);
    $speed.textContent = String(mph).padStart(3, "0");
  } else {
    $speed.textContent = "---";
  }

  $rpm.textContent =
    snap.rpm != null ? String(snap.rpm).padStart(4, " ") : "----";

  if (snap.coolant_c != null) {
    const f = Math.round(snap.coolant_c * 9 / 5 + 32);
    $coolant.textContent = String(f).padStart(3, " ") + "F";
  } else {
    $coolant.textContent = "---F";
  }

  $throttle.textContent =
    snap.throttle_pct != null
      ? String(snap.throttle_pct).padStart(3, " ") + "%"
      : "---%";

  $battery.textContent =
    snap.battery_v != null
      ? snap.battery_v.toFixed(1).padStart(4, " ") + "V"
      : "--.-V";

  $dot.className = "dot " + (snap.connected ? "connected" : "disconnected");
}

// ---- Mock data ----

function startMock() {
  if (useMock) return;
  useMock = true;
  $mockBadge.classList.remove("hidden");

  let t = 0;
  mockTimer = setInterval(() => {
    t += 0.05;
    render({
      rpm: Math.round(800 + 2200 * (0.5 + 0.5 * Math.sin(t * 0.7))),
      speed_kph: Math.round(40 + 80 * (0.5 + 0.5 * Math.sin(t * 0.3))),
      coolant_c: Math.round(85 + 10 * Math.sin(t * 0.1)),
      throttle_pct: Math.round(25 + 50 * (0.5 + 0.5 * Math.sin(t * 0.9))),
      battery_v: 13.2 + 1.2 * Math.sin(t * 0.05),
      connected: true,
    });
  }, 33); // ~30fps
}

function stopMock() {
  if (!useMock) return;
  useMock = false;
  $mockBadge.classList.add("hidden");
  if (mockTimer) {
    clearInterval(mockTimer);
    mockTimer = null;
  }
}

// ---- WebSocket ----

function connectWs() {
  const wsPort = Number(location.port) + WS_PORT_OFFSET;
  const url = `ws://${location.hostname}:${wsPort}`;

  let ws;
  try {
    ws = new WebSocket(url);
  } catch {
    startMock();
    setTimeout(connectWs, 3000);
    return;
  }

  ws.onopen = () => {
    stopMock();
  };

  ws.onmessage = (e) => {
    try {
      const snap = JSON.parse(e.data);
      render(snap);
    } catch { /* ignore bad frames */ }
  };

  ws.onclose = () => {
    startMock();
    setTimeout(connectWs, 2000);
  };

  ws.onerror = () => {
    ws.close();
  };
}

// ---- Boot ----

// If opened as a plain file or on a dev server without the Rust backend,
// go straight to mock mode. Otherwise, try WebSocket.
if (location.protocol === "file:") {
  startMock();
} else {
  connectWs();
  // If WS doesn't connect within 1s, show mock while retrying.
  setTimeout(() => {
    if (!useMock && $dot.classList.contains("disconnected")) {
      startMock();
    }
  }, 1000);
}
