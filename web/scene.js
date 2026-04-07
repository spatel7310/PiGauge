// PiGauge — JDM pixel-art driving scene
//
// Sprite layers (back to front):
//   1. back.png     — sky/mountains (slowest parallax, tiles)
//   2. sun.png      — synthwave sun (static, centered in sky)
//   3. buildings.png — city skyline (medium parallax, tiles)
//   4. palms.png    — palm tree clusters (medium-fast parallax, tiles)
//   5. highway.png  — road surface (fastest parallax, tiles)
//   6. car sprite   — WRX on the road

const canvas = document.getElementById("scene");
const ctx = canvas.getContext("2d");

// ---- State fed by app.js ----
window.sceneState = {
  rpm: 0,
  speed_kph: 0,
  throttle_pct: 0,
  connected: false,
};

// ---- Sprite loading ----
const sprites = {};
const SPRITE_LIST = [
  { key: "back",      src: "background_layers/back.png" },
  { key: "sun",       src: "background_layers/sun.png" },
  { key: "buildings", src: "background_layers/buildings.png" },
  { key: "palms",     src: "background_layers/palms.png" },
  { key: "highway",   src: "background_layers/highway.png" },
  { key: "car_stop",  src: "car_facing_right/wrx_facing_right_stationary.png" },
  { key: "car_drive", src: "car_facing_right/wrx_facing_right_driving.png" },
  { key: "car_speed", src: "car_facing_right/wrx_facing_right_speeding.png" },
];

let spritesLoaded = 0;
for (const { key, src } of SPRITE_LIST) {
  const img = new Image();
  img.onload = () => { sprites[key] = img; spritesLoaded++; };
  img.onerror = () => { sprites[key] = null; spritesLoaded++; };
  img.src = src;
}

// Pick car frame based on speed
function getCarSprite(speedKph) {
  if (speedKph > 80) return sprites["car_speed"];
  if (speedKph > 5)  return sprites["car_drive"];
  return sprites["car_stop"];
}

// ---- Canvas state ----
let W = 480, H = 320;
let lastTime = 0;

// Scroll offsets (in "native" pixels of each sprite)
let scrollBack = 0;
let scrollBuildings = 0;
let scrollPalms = 0;
let scrollHighway = 0;

// Exhaust particles
let exhaustParticles = [];

// ---- Resize ----
function resize() {
  W = window.innerWidth;
  H = window.innerHeight;
  canvas.width = W;
  canvas.height = H;
  ctx.imageSmoothingEnabled = false;
}
window.addEventListener("resize", resize);
resize();

// ---- Tile a sprite horizontally ----
// drawHeight: height in screen px (defaults to full canvas height).
// The sprite is drawn anchored to the bottom of the canvas.
function tileH(img, scrollPx, drawHeight) {
  if (!img) return;
  const dH = drawHeight || H;
  const scale = dH / img.naturalHeight;
  const drawW = Math.ceil(img.naturalWidth * scale);
  const y = H - dH;

  const off = ((scrollPx * scale) % drawW + drawW) % drawW;

  for (let x = -off; x < W + drawW; x += drawW) {
    ctx.drawImage(img, Math.floor(x), Math.floor(y), drawW, Math.ceil(dH));
  }
}

// Draw a single sprite centered horizontally, scaled to screen height
function drawCentered(img) {
  if (!img) return;
  const scale = H / img.naturalHeight;
  const drawW = Math.ceil(img.naturalWidth * scale);
  ctx.drawImage(img, Math.floor((W - drawW) / 2), 0, drawW, H);
}

// ---- Exhaust system ----
function updateExhaust(rpm, x, y) {
  if (rpm < 850) { exhaustParticles = []; return; }
  const intensity = Math.min(1, (rpm - 800) / 5500);

  // Spawn
  if (Math.random() < 0.6 + intensity * 0.4) {
    exhaustParticles.push({
      x: x,
      y: y + (Math.random() - 0.5) * 6,
      vx: -(1.5 + intensity * 4 + Math.random() * 2),
      vy: (Math.random() - 0.5) * 1.0 - 0.4,
      life: 1.0,
      size: Math.floor(2 + Math.random() * 2 + intensity * 2),
    });
  }

  for (let i = exhaustParticles.length - 1; i >= 0; i--) {
    const p = exhaustParticles[i];
    p.x += p.vx;
    p.y += p.vy;
    p.life -= 0.025 + intensity * 0.02;
    if (p.life <= 0) { exhaustParticles.splice(i, 1); continue; }
    const a = p.life * 0.35;
    ctx.fillStyle = `rgba(120, 100, 140, ${a})`;
    ctx.fillRect(Math.floor(p.x), Math.floor(p.y), p.size, p.size);
  }

  if (exhaustParticles.length > 100) exhaustParticles = exhaustParticles.slice(-70);
}

// ---- Speed lines ----
function drawSpeedLines(rpm) {
  if (rpm < 3500) return;
  const intensity = Math.min(1, (rpm - 3500) / 3000);
  const count = Math.floor(intensity * 15);
  const t = performance.now();

  ctx.fillStyle = `rgba(200, 180, 240, ${intensity * 0.12})`;
  for (let i = 0; i < count; i++) {
    const seed = i * 7919 + Math.floor(t / 45) * 3;
    const py = (Math.sin(seed) * 0.5 + 0.5) * H;
    const pw = 25 + intensity * 50 + (Math.sin(seed * 1.3) * 0.5 + 0.5) * 35;
    const px = (Math.sin(seed * 2.7) * 0.5 + 0.5) * W;
    ctx.fillRect(Math.floor(px), Math.floor(py), Math.floor(pw), 1);
  }
}

// ---- Main loop ----
function frame(time) {
  const dt = Math.min(50, time - (lastTime || time)) / 1000;
  lastTime = time;

  const { rpm, speed_kph } = window.sceneState;
  const speedMph = speed_kph * 0.621371;
  const rpmNorm = Math.max(0, (rpm - 700)) / 5800; // 0..1
  const isIdle = speed_kph < 5 && rpm < 1200;

  // Scroll speeds (native sprite pixels per second)
  // These are tuned so layers feel right at various speeds.
  scrollBack      += speedMph * 0.15 * dt;
  scrollBuildings += speedMph * 0.6  * dt;
  scrollPalms     += speedMph * 1.2  * dt;
  scrollHighway   += speedMph * 3.5  * dt;

  // Car rumble — gentle at idle/low speed, aggressive at high RPM
  let rumbleY = 0;
  if (isIdle && rpm > 0) {
    rumbleY = Math.sin(time * 0.025) * 0.4;
  } else if (rpm > 0) {
    const speedFactor = Math.min(1, speedMph / 120);
    const freq = 0.03 + speedFactor * 0.04;
    const amp = 0.3 + rpmNorm * 3.5 + speedFactor * 1.5;
    rumbleY = Math.sin(time * freq) * amp;
  }

  // Screen shake
  document.body.classList.remove("shake-light", "shake-heavy");
  if (rpm > 5500) {
    document.body.classList.add("shake-heavy");
  } else if (rpm > 4000) {
    document.body.classList.add("shake-light");
  }

  // ---- Draw layers back-to-front ----
  ctx.clearRect(0, 0, W, H);

  // 1. Back (sky + mountains) — tiles, slowest
  tileH(sprites["back"], scrollBack, null);

  // 2. Sun — static, centered in upper portion
  drawCentered(sprites["sun"], 0);

  // 3. Buildings — tiles, medium
  tileH(sprites["buildings"], scrollBuildings, null);

  // 4. Palms — tiles, medium-fast
  tileH(sprites["palms"], scrollPalms, null);

  // 5. Speed lines (drawn between BG and road for depth)
  drawSpeedLines(rpm);

  // 6. Highway — tiles, fastest, scaled to ~35% of screen height
  tileH(sprites["highway"], scrollHighway, Math.floor(H * 0.80));

  // 7. Car — pick sprite frame based on speed
  const carImg = getCarSprite(speed_kph);
  if (carImg) {
    const isLandscape = W > H;
    const carW = Math.floor(isLandscape ? W * 0.48 : H * 0.52);
    const scale = carW / carImg.naturalWidth;
    const carH = Math.floor(carImg.naturalHeight * scale);

    const carX = Math.floor(W * 0.15);
    const carBottomTarget = Math.floor(H * 0.97);
    const carY = Math.floor(carBottomTarget - carH + rumbleY);

    ctx.drawImage(carImg, carX, carY, carW, carH);

    // Exhaust comes from rear of car (left side since facing right)
    updateExhaust(rpm, carX + Math.floor(carW * 0.02), carY + Math.floor(carH * 0.65));
  }

  requestAnimationFrame(frame);
} 

requestAnimationFrame(frame);
