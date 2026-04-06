use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use gpio_cdev::{Chip, LineRequestFlags};
use log::{info, warn};

pub struct PowerWatchConfig {
    pub gpio_chip: PathBuf,
    pub gpio_line: u32,
    pub debounce: Duration,
    pub shutdown_delay: Duration,
    pub shutdown_cmd: String,
}

pub fn run(cfg: PowerWatchConfig) -> Result<()> {
    let mut chip = Chip::new(&cfg.gpio_chip)
        .with_context(|| format!("open gpio chip {}", cfg.gpio_chip.display()))?;

    let line = chip
        .get_line(cfg.gpio_line)
        .with_context(|| format!("get gpio line {}", cfg.gpio_line))?;

    let handle = line
        .request(LineRequestFlags::INPUT, 0, "pigauge-power")
        .with_context(|| format!("request gpio line {} as input", cfg.gpio_line))?;

    info!(
        "power-watch started: chip={} line={} debounce={:?} delay={:?} cmd={}",
        cfg.gpio_chip.display(),
        cfg.gpio_line,
        cfg.debounce,
        cfg.shutdown_delay,
        cfg.shutdown_cmd
    );

    // Convention: GPIO high == external power present.
    let mut last_high = handle.get_value().unwrap_or(1) == 1;
    let mut low_since: Option<Instant> = None;

    loop {
        let high = match handle.get_value() {
            Ok(v) => v == 1,
            Err(e) => {
                warn!("gpio read error: {e:#}");
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
        };

        if high {
            low_since = None;
        } else if low_since.is_none() {
            low_since = Some(Instant::now());
        }

        if !high {
            if let Some(t0) = low_since {
                if t0.elapsed() >= cfg.debounce {
                    info!(
                        "power-loss confirmed; waiting {:?} then shutting down",
                        cfg.shutdown_delay
                    );
                    std::thread::sleep(cfg.shutdown_delay);
                    exec_shutdown(&cfg.shutdown_cmd)?;
                    std::thread::sleep(Duration::from_secs(10));
                }
            }
        }

        if high != last_high {
            info!(
                "power state changed: {}",
                if high { "PRESENT" } else { "LOST" }
            );
            last_high = high;
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

fn exec_shutdown(cmd: &str) -> Result<()> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .status()
        .with_context(|| format!("exec shutdown cmd {cmd:?}"))?;
    if !status.success() {
        bail!("shutdown cmd failed with {status}");
    }
    Ok(())
}
