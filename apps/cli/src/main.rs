use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use horiplay_core::percentiles;
use horiplay_session::{Action, Config, Engine};
use std::time::{Duration, Instant};
#[derive(Parser)]
#[command(
    version,
    about = "HoriPlay alpha: native controller diagnostics and encrypted remote gamepad sessions"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Native SDL3 detection and state monitor. No generated controller inputs.
    GamepadTest {
        #[arg(long, default_value_t = 10)]
        seconds: u64,
    },
    /// Independent Windows XInput observer; use alongside a game/remote host.
    XinputTest {
        #[arg(long, default_value_t = 10)]
        seconds: u64,
    },
    /// Create/join a controller session. Commands on stdin: accept, grant ID SLOT, revoke, status, stop.
    Host {
        #[arg(long, default_value = "ws://127.0.0.1:8787/ws")]
        signaling: String,
        #[arg(long)]
        ice: Vec<String>,
    },
    Join {
        invite: String,
        #[arg(long, default_value = "ws://127.0.0.1:8787/ws")]
        signaling: String,
        #[arg(long)]
        ice: Vec<String>,
    },
    Doctor,
    /// Same-machine actual WebRTC echo timing with synthetic 56-byte frames, not hardware input latency.
    TransportBench {
        #[arg(long, default_value_t = 300)]
        samples: usize,
    },
}
#[tokio::main]
async fn main() -> Result<()> {
    match Args::parse().command {
        Command::TransportBench { samples } => println!(
            "{}",
            serde_json::to_string_pretty(&horiplay_networking::loopback_benchmark(samples).await?)?
        ),
        Command::Doctor => {
            let path = horiplay_gamepad::SdlGamepads::default_path()?;
            println!("HoriPlay {}\nOS: {} {}\nSDL3: {} (exists: {})\nVirtual host backend: Windows VIIPER, separate installation and user-approved signed USB/IP driver required.\nMedia: unavailable in this input alpha. No FPS or gameplay claim.",env!("CARGO_PKG_VERSION"),std::env::consts::OS,std::env::consts::ARCH,path.display(),path.exists());
            #[cfg(windows)]
            {
                let x = horiplay_virtual_gamepad::xinput::XInput::load()?;
                for i in 0..4 {
                    println!("XInput slot {}: {:?}", i + 1, x.read(i));
                }
            }
        }
        Command::GamepadTest { seconds } => {
            let path = horiplay_gamepad::SdlGamepads::default_path()?;
            let mut pads = horiplay_gamepad::SdlGamepads::load(&path)?;
            let start = Instant::now();
            let mut previous = String::new();
            let mut times = Vec::new();
            while start.elapsed() < Duration::from_secs(seconds) {
                let begin = Instant::now();
                let devices = pads.poll()?;
                times.push(begin.elapsed().as_secs_f64() * 1000.0);
                let json = serde_json::to_string(&devices)?;
                if json != previous {
                    println!("{json}");
                    previous = json;
                }
                std::thread::sleep(Duration::from_millis(4));
            }
            println!(
                "SDL poll work time (excludes device/sample interval): {}",
                serde_json::to_string(&percentiles(&times))?
            );
        }
        Command::XinputTest { seconds } => {
            #[cfg(windows)]
            {
                let x = horiplay_virtual_gamepad::xinput::XInput::load()?;
                let start = Instant::now();
                let mut previous = String::new();
                while start.elapsed() < Duration::from_secs(seconds) {
                    let value = serde_json::to_string(
                        &(0..4).map(|slot| x.read(slot)).collect::<Vec<_>>(),
                    )?;
                    if value != previous {
                        println!("{value}");
                        previous = value;
                    }
                    std::thread::sleep(Duration::from_millis(4));
                }
            }
            #[cfg(not(windows))]
            {
                let _ = seconds;
                anyhow::bail!("XInput observation is Windows-only");
            }
        }
        Command::Host { signaling, ice } => session(signaling, ice, None).await?,
        Command::Join {
            invite,
            signaling,
            ice,
        } => session(signaling, ice, Some(invite)).await?,
    }
    Ok(())
}
async fn session(signaling: String, ice: Vec<String>, invite: Option<String>) -> Result<()> {
    let engine = Engine::new();
    let mut config = Config {
        signaling,
        ..Default::default()
    };
    if !ice.is_empty() {
        config.ice_urls = ice;
    }
    config.turn_username = std::env::var("HORIPLAY_TURN_USER").unwrap_or_default();
    config.turn_password = std::env::var("HORIPLAY_TURN_PASSWORD").unwrap_or_default();
    engine.start(config, invite).await?;
    if let Some(code) = engine.snapshot().invite {
        println!("Invite (expires for new joins after 10 min; keep private): {code}");
    }
    println!("Commands: accept | grant <controller-id> <slot 1..4> | revoke | status | stop");
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    std::thread::spawn(move || {
        for line in std::io::stdin().lines() {
            match line {
                Ok(line) => {
                    if tx.blocking_send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    let mut timer = tokio::time::interval(Duration::from_secs(2));
    let mut last = String::new();
    loop {
        tokio::select! {
                    _=tokio::signal::ctrl_c()=>{let _=engine.action(Action::Stop).await;break;},
                    _=timer.tick()=>{let s=engine.snapshot();let status=format!("{} | local={} | guest={:?} | remote={:?} | backend={:?} | error={:?}",s.phase,s.devices.len(),s.guest_name,s.remote,s.backend,s.error);if status!=last{println!("{status}");last=status;}
        if s.phase=="Stopped"{break;}},
                    Some(line)=rx.recv()=>{let parts:Vec<_>=line.split_whitespace().collect();let action=match parts.as_slice(){["accept"]=>Some(Action::AcceptPeer),["revoke"]=>Some(Action::Revoke),["stop"]=>Some(Action::Stop),["grant",id,slot]=>{let slot:u8=slot.parse().context("slot number")?;if !(1..=4).contains(&slot){anyhow::bail!("slot must be 1..4");}Some(Action::Grant{controller:id.parse()?,slot:slot-1})},["status"]=>{println!("{}",serde_json::to_string_pretty(&engine.snapshot())?);None},_=>{println!("Unknown command");None}};
                        if let Some(action)=action{engine.action(action).await?;}
                    }
                }
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    Ok(())
}
