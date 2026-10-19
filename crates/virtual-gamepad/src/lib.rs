use anyhow::{bail, Context, Result};
use horiplay_core::PadState;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackendStatus {
    pub backend: String,
    pub device_created: bool,
    pub os_slot: Option<u8>,
    pub input_observed: bool,
    pub game_verified: bool,
    pub rumble_observed: bool,
    pub detail: String,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rumble {
    pub low: u16,
    pub high: u16,
    pub duration_ms: u32,
}
pub trait VirtualGamepadBackend: Send {
    fn update(&mut self, state: PadState) -> Result<()>;
    fn neutralize(&mut self) -> Result<()> {
        self.update(PadState::default())
    }
    fn feedback(&mut self) -> Result<Option<Rumble>>;
    fn status(&self) -> BackendStatus;
}

/// VIIPER runs as an independently installed process (GPL-3.0+). No GPL library is linked.
/// The raw feeder API is restricted to loopback; Internet transport is WebRTC DTLS/SCTP.
pub struct Viiper {
    address: SocketAddr,
    bus: u64,
    stream: TcpStream,
    status: BackendStatus,
    feedback_bytes: Vec<u8>,
    #[cfg(windows)]
    expected_slot: u8,
    #[cfg(windows)]
    probe: xinput::XInput,
}
pub fn xbox_report(state: PadState) -> [u8; 20] {
    const MAP: [u32; 15] = [
        0x1000, 0x2000, 0x4000, 0x8000, 0x20, 0x400, 0x10, 0x40, 0x80, 0x100, 0x200, 1, 2, 4, 8,
    ];
    let mut report = [0; 20];
    let buttons = MAP.iter().enumerate().fold(0u32, |b, (i, flag)| {
        if state.buttons & (1 << i) != 0 {
            b | flag
        } else {
            b
        }
    });
    report[..4].copy_from_slice(&buttons.to_le_bytes());
    for i in 0..2 {
        report[4 + i] = ((state.triggers[i].min(32767) as u32 * 255 + 16383) / 32767) as u8;
    }
    for i in 0..4 {
        let axis = if i % 2 == 1 {
            state.axes[i].saturating_neg()
        } else {
            state.axes[i]
        };
        report[6 + i * 2..8 + i * 2].copy_from_slice(&axis.to_le_bytes());
    }
    report
}
fn request(address: SocketAddr, command: &str) -> Result<serde_json::Value> {
    let mut s=TcpStream::connect_timeout(&address,Duration::from_millis(700)).context("VIIPER not running on loopback. Install its signed USB/IP prerequisite explicitly; then start VIIPER.")?;
    s.set_read_timeout(Some(Duration::from_secs(2)))?;
    s.set_write_timeout(Some(Duration::from_millis(100)))?;
    s.set_nodelay(true)?;
    s.write_all(command.as_bytes())?;
    s.write_all(&[0])?;
    let mut bytes = Vec::new();
    s.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        bail!("VIIPER response too large");
    }
    let v: serde_json::Value = serde_json::from_slice(&bytes)?;
    if v.get("status")
        .and_then(|v| v.as_u64())
        .is_some_and(|s| s >= 400)
    {
        bail!("VIIPER: {v}");
    }
    Ok(v)
}
impl Viiper {
    pub fn connect(address: SocketAddr, slot: u8) -> Result<Self> {
        if !address.ip().is_loopback() || slot > 3 {
            bail!("VIIPER requires a loopback address and slot 0..3");
        }
        if !cfg!(windows) {
            bail!(
                "Windows host virtual controller only; macOS host backend has not been validated"
            );
        }
        #[cfg(windows)]
        let probe = xinput::XInput::load()?;
        #[cfg(windows)]
        if probe.read(slot).is_some() {
            bail!(
                "requested XInput slot {} is already occupied; select a free slot",
                slot + 1
            );
        }
        let version = request(address, "ping")?;
        if version["server"] != "VIIPER" {
            bail!("unexpected virtual controller service");
        }
        let created = request(address, "bus/create")?;
        let bus = created["busId"].as_u64().context("missing VIIPER busId")?;
        let result = (|| {
            let dev = request(address, &format!("bus/{bus}/add {{\"type\":\"xbox360\"}}"))?;
            let id = dev["devId"].as_str().context("missing device ID")?;
            if !id.chars().all(|c| c.is_ascii_digit() || c == '-') {
                bail!("invalid device ID");
            }
            let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(700))?;
            stream.set_nodelay(true)?;
            stream.set_write_timeout(Some(Duration::from_millis(10)))?;
            stream.write_all(format!("bus/{bus}/{id}\0").as_bytes())?;
            stream.set_nonblocking(true)?;
            #[cfg(windows)]
            {
                let started = std::time::Instant::now();
                while probe.read(slot).is_none() {
                    if started.elapsed() > Duration::from_secs(3) {
                        bail!("VIIPER API created a device, but requested XInput slot {} did not appear. Verify signed USB/IP driver, local auto-attach, and Windows player-index assignment. No input permission was granted.",slot+1);
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
            Ok(Self{address,bus,stream,status:BackendStatus{backend:"VIIPER / USBIP Xbox 360".into(),device_created:true,os_slot:Some(slot),detail:"New XInput device observed in requested slot. Waiting for a non-neutral input match; real game acceptance pending.".into(),..Default::default()},feedback_bytes:Vec::new(),#[cfg(windows)] expected_slot:slot,#[cfg(windows)] probe})
        })();
        if result.is_err() {
            let _ = request(address, &format!("bus/remove {bus}"));
        }
        result
    }
}
impl VirtualGamepadBackend for Viiper {
    fn update(&mut self, state: PadState) -> Result<()> {
        if !state.valid() {
            bail!("invalid controller report");
        }
        let report = xbox_report(state);
        // Twenty-byte writes never enter an application queue. Partial/WouldBlock is fatal:
        // continuing could splice report boundaries or deliver stale input later.
        match self.stream.write(&report) {
            Ok(20) => {}
            Ok(_) => bail!("partial VIIPER report; disconnecting to preserve framing"),
            Err(e) => return Err(e.into()),
        }
        #[cfg(windows)]
        if let Some(observed) = self.probe.read(self.expected_slot) {
            self.status.os_slot = Some(self.expected_slot);
            if !state.neutral() && observed == xinput::expected(state) {
                self.status.input_observed = true;
                self.status.detail="Independent XInput read matched a non-neutral report. Actual game compatibility still needs hardware acceptance.".into();
            }
        }
        Ok(())
    }
    fn feedback(&mut self) -> Result<Option<Rumble>> {
        let mut buf = [0; 64];
        loop {
            match self.stream.read(&mut buf) {
                Ok(0) => bail!("VIIPER stream closed"),
                Ok(n) => {
                    self.feedback_bytes.extend_from_slice(&buf[..n]);
                    if self.feedback_bytes.len() > 4096 {
                        bail!("feedback overflow");
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.into()),
            }
        }
        if self.feedback_bytes.len() < 2 {
            return Ok(None);
        }
        let complete = self.feedback_bytes.len() / 2 * 2;
        let low = self.feedback_bytes[complete - 2] as u16 * 257;
        let high = self.feedback_bytes[complete - 1] as u16 * 257;
        self.feedback_bytes.drain(..complete);
        self.status.rumble_observed = true;
        Ok(Some(Rumble {
            low,
            high,
            duration_ms: 250,
        }))
    }
    fn status(&self) -> BackendStatus {
        self.status.clone()
    }
}
impl Drop for Viiper {
    fn drop(&mut self) {
        let _ = self.neutralize();
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
        let _ = request(self.address, &format!("bus/remove {}", self.bus));
    }
}

#[cfg(windows)]
pub mod xinput {
    use super::*;
    #[repr(C)]
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
    pub struct Gamepad {
        pub buttons: u16,
        pub left_trigger: u8,
        pub right_trigger: u8,
        pub lx: i16,
        pub ly: i16,
        pub rx: i16,
        pub ry: i16,
    }
    #[repr(C)]
    #[derive(Default)]
    struct State {
        packet: u32,
        gamepad: Gamepad,
    }
    pub struct XInput {
        _lib: libloading::Library,
        get: unsafe extern "system" fn(u32, *mut State) -> u32,
    }
    impl XInput {
        pub fn load() -> Result<Self> {
            let root = std::env::var_os("SystemRoot").context("SystemRoot unavailable")?;
            let path = std::path::Path::new(&root).join("System32/xinput1_4.dll");
            unsafe {
                let lib = libloading::Library::new(path)?;
                let get = *lib.get(b"XInputGetState\0")?;
                Ok(Self { _lib: lib, get })
            }
        }
        pub fn read(&self, slot: u8) -> Option<Gamepad> {
            let mut s = State::default();
            if unsafe { (self.get)(slot as u32, &mut s) } == 0 {
                Some(s.gamepad)
            } else {
                None
            }
        }
    }
    pub fn expected(state: PadState) -> Gamepad {
        let r = xbox_report(state);
        Gamepad {
            buttons: u16::from_le_bytes([r[0], r[1]]),
            left_trigger: r[4],
            right_trigger: r[5],
            lx: i16::from_le_bytes([r[6], r[7]]),
            ly: i16::from_le_bytes([r[8], r[9]]),
            rx: i16::from_le_bytes([r[10], r[11]]),
            ry: i16::from_le_bytes([r[12], r[13]]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xbox_axes_buttons_and_triggers() {
        let p = PadState {
            buttons: 1 | (1 << 6) | (1 << 11),
            axes: [6553, -16384, -32768, 32767],
            triggers: [16384, 32767],
        };
        let b = xbox_report(p);
        assert_eq!(u32::from_le_bytes(b[..4].try_into().unwrap()), 0x1011);
        assert_eq!(&b[4..6], &[128, 255]);
        assert_eq!(i16::from_le_bytes([b[8], b[9]]), 16384);
        assert_eq!(&b[14..], &[0; 6]);
    }
    #[test]
    fn backend_rejects_remote_api() {
        assert!(Viiper::connect("192.0.2.1:3242".parse().unwrap(), 1).is_err());
    }
}
