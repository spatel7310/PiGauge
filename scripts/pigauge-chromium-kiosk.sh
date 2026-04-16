#!/bin/bash
# PiGauge kiosk launcher with boot logging.
# Log file: ~/.local/share/pigauge/kiosk-boot.log

LOG_DIR="$HOME/.local/share/pigauge"
LOG="$LOG_DIR/kiosk-boot.log"
mkdir -p "$LOG_DIR"

# Keep last 5 runs (rotate before appending)
if [ -f "$LOG" ]; then
    mv "$LOG" "$LOG.1"
fi

exec >> "$LOG" 2>&1

echo "=== PiGauge kiosk boot $(date '+%Y-%m-%d %H:%M:%S') ==="
echo "USER=$USER DISPLAY=$DISPLAY WAYLAND_DISPLAY=$WAYLAND_DISPLAY XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR"

echo "[$(date '+%T.%3N')] Waiting for compositor / display transform..."

# Wait for wlr-randr to be available
WLR_WAIT=0
until wlr-randr >/dev/null 2>&1; do
    sleep 0.5
    WLR_WAIT=$((WLR_WAIT + 1))
    if [ $WLR_WAIT -ge 30 ]; then
        echo "[$(date '+%T.%3N')] WARN: wlr-randr never responded after 15s, proceeding anyway"
        break
    fi
done
echo "[$(date '+%T.%3N')] wlr-randr available (waited ${WLR_WAIT}x0.5s)"

# Log current display state
echo "[$(date '+%T.%3N')] wlr-randr output:"
wlr-randr 2>&1 | sed 's/^/  /'

# Wait for portrait rotation (Transform: 90)
TRANSFORM_WAIT=0
until wlr-randr 2>/dev/null | grep -q 'Transform: 90'; do
    sleep 0.5
    TRANSFORM_WAIT=$((TRANSFORM_WAIT + 1))
    if [ $TRANSFORM_WAIT -ge 30 ]; then
        echo "[$(date '+%T.%3N')] WARN: Transform: 90 never appeared after 15s, proceeding anyway"
        echo "[$(date '+%T.%3N')] Final wlr-randr output:"
        wlr-randr 2>&1 | sed 's/^/  /'
        break
    fi
done
echo "[$(date '+%T.%3N')] Portrait transform confirmed (waited ${TRANSFORM_WAIT}x0.5s)"

# Brief settle delay
sleep 0.5
echo "[$(date '+%T.%3N')] Launching Chromium..."

exec /usr/bin/chromium \
    --app=http://localhost:8080 \
    --start-fullscreen \
    --noerrdialogs \
    --disable-infobars \
    --disable-session-crashed-bubble \
    --disable-translate \
    --no-first-run \
    --password-store=basic \
    --disk-cache-size=1
