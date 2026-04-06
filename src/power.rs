 use std::{
     path::PathBuf,
     process::Command,
     time::{Duration, Instant},
 };
 
 use anyhow::{bail, Context, Result};
 use log::{info, warn};
 
 pub struct PowerWatchConfig {
     pub gpio_chip: PathBuf,
     pub gpio_line: u32,
     pub debounce: Duration,
     pub shutdown_delay: Duration,
     pub shutdown_cmd: String,
 }
 
 pub fn run(cfg: PowerWatchConfig) -> Result<()> {
     // We use libgpiod via its CLI (`gpioget`) only if available? No—prefer pure sysfs? sysfs is deprecated.
     // To keep dependencies light, we'll use /dev/gpiochip with the `gpio-cdev` ioctl via `gpio-cdev` crate
     // ... but we didn't add it. So instead, implement a minimal poll using `gpioget` is undesirable.
     //
     // Practical compromise: read from /sys/class/gpio if present; on Raspberry Pi OS it still works.
     // If you run Bookworm with newer kernels, you can enable sysfs gpio via boot config; otherwise we'll add gpio-cdev later.
 
    let _ = cfg.gpio_chip; // Reserved for future gpio-cdev implementation.
    let line_path = export_and_get_value_path(cfg.gpio_line)?;
 
     info!(
         "power-watch started: line={} debounce={:?} delay={:?} cmd={}",
         cfg.gpio_line, cfg.debounce, cfg.shutdown_delay, cfg.shutdown_cmd
     );
 
     // Convention: GPIO high == external power present.
     let mut last_high = read_gpio_value(&line_path).unwrap_or(true);
     let mut low_since: Option<Instant> = None;
 
     loop {
         let high = match read_gpio_value(&line_path) {
             Ok(v) => v,
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
                     info!("power-loss confirmed; waiting {:?} then shutting down", cfg.shutdown_delay);
                     std::thread::sleep(cfg.shutdown_delay);
                     exec_shutdown(&cfg.shutdown_cmd)?;
                     // If shutdown doesn't halt immediately, sleep to avoid loops.
                     std::thread::sleep(Duration::from_secs(10));
                 }
             }
         }
 
         // Log edges occasionally.
         if high != last_high {
             info!("power state changed: {}", if high { "PRESENT" } else { "LOST" });
             last_high = high;
         }
 
         std::thread::sleep(Duration::from_millis(100));
     }
 }
 
 fn exec_shutdown(cmd: &str) -> Result<()> {
     // Execute via sh -c to support args.
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
 
 fn export_and_get_value_path(line: u32) -> Result<std::path::PathBuf> {
     let base = std::path::Path::new("/sys/class/gpio");
     if !base.exists() {
         bail!("/sys/class/gpio not available; need gpio-cdev support");
     }
 
     let gpio_dir = base.join(format!("gpio{line}"));
     if !gpio_dir.exists() {
         std::fs::write(base.join("export"), format!("{line}"))
             .with_context(|| format!("export gpio {line}"))?;
         // Give udev/sysfs a moment.
         std::thread::sleep(Duration::from_millis(80));
     }
 
     // Configure as input.
     let dir_path = gpio_dir.join("direction");
     if dir_path.exists() {
         let _ = std::fs::write(&dir_path, "in");
     }
 
     Ok(gpio_dir.join("value"))
 }
 
 fn read_gpio_value(value_path: &std::path::Path) -> Result<bool> {
     let v = std::fs::read_to_string(value_path).context("read gpio value")?;
     Ok(v.trim() == "1")
 }
 
