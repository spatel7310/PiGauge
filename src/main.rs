 use std::{path::PathBuf, time::Duration};
 
 use anyhow::{Context, Result};
 use clap::{Parser, Subcommand};
 
 mod obd;
 mod power;
 mod ui;
 
 #[derive(Parser, Debug)]
 #[command(name = "pigauge", version, about = "Fast minimal OBD2 dashboard + safe shutdown watcher")]
 struct Cli {
     #[command(subcommand)]
     cmd: Command,
 }
 
 #[derive(Subcommand, Debug)]
 enum Command {
     /// Run the on-screen gauge UI and poll OBD2.
     Ui {
         /// Serial device for your USB OBD adapter (e.g. /dev/ttyUSB0).
         #[arg(long, env = "PIGAUGE_OBD_DEV")]
         obd_dev: PathBuf,
 
         /// Serial baud rate (common: 38400, 9600, 115200). Try 38400 first for ELM327.
         #[arg(long, default_value_t = 38400, env = "PIGAUGE_OBD_BAUD")]
         obd_baud: u32,
 
         /// UI target FPS.
         #[arg(long, default_value_t = 30, env = "PIGAUGE_FPS")]
         fps: u32,
 
         /// Poll interval for OBD requests (ms).
         #[arg(long, default_value_t = 200, env = "PIGAUGE_POLL_MS")]
         poll_ms: u64,
     },
 
     /// Watch a GPIO line for power-loss and trigger shutdown.
     ///
     /// Use this when you can wire "external 5V present" to a GPIO (via level shifting/voltage divider),
     /// or if your UPS HAT exposes a power-good signal on a pin.
     PowerWatch {
         /// GPIO line number (libgpiod line, not BCM pin). On Pi this usually matches BCM number, but verify.
         #[arg(long, env = "PIGAUGE_GPIO_LINE")]
         gpio_line: u32,
 
         /// GPIO chip path (e.g. /dev/gpiochip0).
         #[arg(long, default_value = "/dev/gpiochip0", env = "PIGAUGE_GPIO_CHIP")]
         gpio_chip: PathBuf,
 
         /// Debounce time (ms) to confirm power is truly gone.
         #[arg(long, default_value_t = 1500, env = "PIGAUGE_DEBOUNCE_MS")]
         debounce_ms: u64,
 
         /// Delay (ms) after confirming power-loss before shutdown (gives time to finish writes).
         #[arg(long, default_value_t = 1500, env = "PIGAUGE_SHUTDOWN_DELAY_MS")]
         shutdown_delay_ms: u64,
 
         /// Command to execute for shutdown.
         #[arg(long, default_value = "/sbin/shutdown -h now", env = "PIGAUGE_SHUTDOWN_CMD")]
         shutdown_cmd: String,
     },
 }
 
 fn main() -> Result<()> {
     env_logger::init();
     let cli = Cli::parse();
 
     match cli.cmd {
         Command::Ui {
             obd_dev,
             obd_baud,
             fps,
             poll_ms,
         } => {
             let poll = Duration::from_millis(poll_ms);
             let cfg = ui::UiConfig {
                 obd_dev,
                 obd_baud,
                 fps,
                 poll_interval: poll,
             };
             ui::run(cfg).context("ui mode failed")
         }
         Command::PowerWatch {
             gpio_line,
             gpio_chip,
             debounce_ms,
             shutdown_delay_ms,
             shutdown_cmd,
         } => power::run(power::PowerWatchConfig {
             gpio_chip,
             gpio_line,
             debounce: Duration::from_millis(debounce_ms),
             shutdown_delay: Duration::from_millis(shutdown_delay_ms),
             shutdown_cmd,
         })
         .context("power-watch mode failed"),
     }
 }
 
