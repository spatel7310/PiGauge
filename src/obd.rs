 use std::{
     io::{Read, Write},
     time::{Duration, Instant},
 };
 
 use anyhow::{anyhow, bail, Context, Result};
 
 #[derive(Debug, Clone, Copy, Default)]
 pub struct ObdSnapshot {
     pub rpm: Option<u16>,
     pub speed_kph: Option<u8>,
     pub coolant_c: Option<i16>,
     pub throttle_pct: Option<u8>,
     pub battery_v: Option<f32>,
     pub last_ok: Option<Instant>,
 }
 
 #[derive(Debug, Clone)]
 pub struct ObdConfig {
     pub dev: std::path::PathBuf,
     pub baud: u32,
     pub timeout: Duration,
 }
 
 pub struct Elm327 {
     port: Box<dyn serialport::SerialPort>,
     rx_buf: Vec<u8>,
 }
 
 impl Elm327 {
     pub fn open(cfg: &ObdConfig) -> Result<Self> {
        let port = serialport::new(cfg.dev.to_string_lossy(), cfg.baud)
             .timeout(cfg.timeout)
             .open()
             .with_context(|| format!("open serial {}", cfg.dev.display()))?;
         Ok(Self {
             port,
             rx_buf: vec![0u8; 4096],
         })
     }
 
     pub fn init_auto(&mut self) -> Result<()> {
         // Typical low-latency, minimal formatting.
         // Many adapters need a bit after reset.
         self.cmd("ATZ")?;
         std::thread::sleep(Duration::from_millis(800));
         let _ = self.drain_for(Duration::from_millis(300));
 
         self.cmd("ATE0")?; // echo off
         self.cmd("ATL0")?; // linefeeds off
         self.cmd("ATS0")?; // spaces off
         self.cmd("ATH0")?; // headers off
         self.cmd("ATSP0")?; // automatic protocol
         self.cmd("ATAT1")?; // adaptive timing on
         self.cmd("ATST0A")?; // ~40ms timeout units; tune later
         Ok(())
     }
 
     pub fn read_snapshot(&mut self) -> Result<ObdSnapshot> {
         // Query a small set of common PIDs; keep it cheap.
         let mut snap = ObdSnapshot::default();
 
         if let Ok(v) = self.query_pid_u16("010C", parse_rpm) {
             snap.rpm = Some(v);
             snap.last_ok = Some(Instant::now());
         }
         if let Ok(v) = self.query_pid_u8("010D", parse_speed) {
             snap.speed_kph = Some(v);
             snap.last_ok = Some(Instant::now());
         }
         if let Ok(v) = self.query_pid_i16("0105", parse_temp_coolant) {
             snap.coolant_c = Some(v);
             snap.last_ok = Some(Instant::now());
         }
         if let Ok(v) = self.query_pid_u8("0111", parse_throttle) {
             snap.throttle_pct = Some(v);
             snap.last_ok = Some(Instant::now());
         }
         if let Ok(v) = self.query_pid_f32("ATRV", parse_voltage) {
             snap.battery_v = Some(v);
             snap.last_ok = Some(Instant::now());
         }
 
         Ok(snap)
     }
 
     fn cmd(&mut self, cmd: &str) -> Result<String> {
         self.send_line(cmd)?;
         self.read_until_prompt(Duration::from_millis(900))
             .with_context(|| format!("cmd {cmd}"))
     }
 
     fn query_pid_u16(&mut self, q: &str, f: fn(&[u8]) -> Result<u16>) -> Result<u16> {
         self.send_line(q)?;
         let raw = self.read_until_prompt(Duration::from_millis(900))?;
         let bytes = extract_hex_bytes(&raw)?;
         f(&bytes)
     }
 
     fn query_pid_u8(&mut self, q: &str, f: fn(&[u8]) -> Result<u8>) -> Result<u8> {
         self.send_line(q)?;
         let raw = self.read_until_prompt(Duration::from_millis(900))?;
         let bytes = extract_hex_bytes(&raw)?;
         f(&bytes)
     }
 
     fn query_pid_i16(&mut self, q: &str, f: fn(&[u8]) -> Result<i16>) -> Result<i16> {
         self.send_line(q)?;
         let raw = self.read_until_prompt(Duration::from_millis(900))?;
         let bytes = extract_hex_bytes(&raw)?;
         f(&bytes)
     }
 
     fn query_pid_f32(&mut self, q: &str, f: fn(&str) -> Result<f32>) -> Result<f32> {
         self.send_line(q)?;
         let raw = self.read_until_prompt(Duration::from_millis(900))?;
         f(raw.as_str())
     }
 
     fn send_line(&mut self, s: &str) -> Result<()> {
         // ELM expects \r line endings.
         self.port
             .write_all(format!("{s}\r").as_bytes())
             .context("serial write")?;
         self.port.flush().ok();
         Ok(())
     }
 
     fn read_until_prompt(&mut self, max_wait: Duration) -> Result<String> {
         let start = Instant::now();
         let mut out = Vec::<u8>::with_capacity(256);
         loop {
             if start.elapsed() > max_wait {
                 bail!("timeout waiting for prompt");
             }
 
             match self.port.read(&mut self.rx_buf) {
                 Ok(0) => {}
                 Ok(n) => {
                     out.extend_from_slice(&self.rx_buf[..n]);
                     if out.contains(&b'>') {
                         break;
                     }
                 }
                 Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                 Err(e) => return Err(e).context("serial read"),
             }
         }
         let s = String::from_utf8_lossy(&out).to_string();
         Ok(clean_elm_text(&s))
     }
 
     fn drain_for(&mut self, dur: Duration) -> usize {
         let start = Instant::now();
         let mut bytes = 0usize;
         while start.elapsed() < dur {
             match self.port.read(&mut self.rx_buf) {
                 Ok(n) => bytes += n,
                 Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                 Err(_) => break,
             }
         }
         bytes
     }
 }
 
 fn clean_elm_text(s: &str) -> String {
     // Strip prompt, CR/LF, and common noise lines.
     let mut t = s.replace('>', "");
     t = t.replace('\r', "\n");
     t = t.replace('\n', "\n");
     t.lines()
         .map(|l| l.trim())
         .filter(|l| !l.is_empty())
         .filter(|l| !l.eq_ignore_ascii_case("SEARCHING..."))
         .filter(|l| !l.eq_ignore_ascii_case("STOPPED"))
         .collect::<Vec<_>>()
         .join("\n")
 }
 
 fn extract_hex_bytes(s: &str) -> Result<Vec<u8>> {
     // ELM output may include multiple lines. We pick all hex byte tokens.
     // Example: "41 0C 1A F8" or "410C1AF8"
     let mut bytes = Vec::<u8>::new();
     for tok in s
         .split(|c: char| c.is_whitespace() || c == '\n' || c == '\r')
         .filter(|t| !t.is_empty())
     {
         let t = tok.trim();
         if t.len() == 2 && t.chars().all(|c| c.is_ascii_hexdigit()) {
             bytes.push(u8::from_str_radix(t, 16).context("hex byte")?);
             continue;
         }
         if t.len() % 2 == 0 && t.chars().all(|c| c.is_ascii_hexdigit()) && t.len() >= 4 {
             // Chunk into bytes.
             for i in (0..t.len()).step_by(2) {
                 let b = u8::from_str_radix(&t[i..i + 2], 16).context("hex chunk")?;
                 bytes.push(b);
             }
         }
     }
     if bytes.is_empty() {
         Err(anyhow!("no hex bytes found in response: {s:?}"))
     } else {
         Ok(bytes)
     }
 }
 
 // Parsers expect the payload for Mode 01 PID xx:
 // response "41 xx A B ..." so bytes[0]=0x41, bytes[1]=PID.
 fn ensure_mode01_pid(bytes: &[u8], pid: u8) -> Result<&[u8]> {
     if bytes.len() < 3 {
         bail!("short response: {bytes:?}");
     }
     if bytes[0] != 0x41 || bytes[1] != pid {
         bail!("unexpected response header: {bytes:?}");
     }
     Ok(&bytes[2..])
 }
 
 fn parse_rpm(bytes: &[u8]) -> Result<u16> {
     let p = ensure_mode01_pid(bytes, 0x0C)?;
     if p.len() < 2 {
         bail!("rpm missing bytes: {bytes:?}");
     }
     let a = p[0] as u16;
     let b = p[1] as u16;
     Ok(((a * 256) + b) / 4)
 }
 
 fn parse_speed(bytes: &[u8]) -> Result<u8> {
     let p = ensure_mode01_pid(bytes, 0x0D)?;
     Ok(p[0])
 }
 
 fn parse_temp_coolant(bytes: &[u8]) -> Result<i16> {
     let p = ensure_mode01_pid(bytes, 0x05)?;
     Ok((p[0] as i16) - 40)
 }
 
 fn parse_throttle(bytes: &[u8]) -> Result<u8> {
     let p = ensure_mode01_pid(bytes, 0x11)?;
     Ok(((p[0] as u16) * 100 / 255) as u8)
 }
 
 fn parse_voltage(s: &str) -> Result<f32> {
     // ATRV output: "12.4V" (sometimes with spaces/newlines)
     let cleaned = s
         .lines()
         .map(|l| l.trim())
         .find(|l| l.to_ascii_lowercase().contains('v'))
         .unwrap_or(s.trim());
 
     let cleaned = cleaned.trim().trim_end_matches(|c: char| c == 'V' || c == 'v');
     cleaned
         .parse::<f32>()
         .with_context(|| format!("parse voltage from {s:?}"))
 }
 
