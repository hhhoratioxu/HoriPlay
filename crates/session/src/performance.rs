//! Explicit synthetic diagnostics, isolated from Engine/device discovery and virtual drivers.
use crate::input::{GuestInput, InputFeed, Sample};
use anyhow::{bail, Context, Result};
use horiplay_core::{percentiles, InputSettings, Owner, PadState, Percentiles};
use horiplay_input_transport::{InputPacket, Receiver};
use horiplay_networking::{loopback_pair, PathInfo, PeerEvent};
use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct InputEvidence {
    pub scheduling: String,
    pub attempted: usize,
    pub accepted: usize,
    pub sample_to_authorized_receiver: Percentiles,
    pub wait_before_send: Percentiles,
    pub path: PathInfo,
    pub scope: String,
}

/// The two modes use identical production mailbox, sender, DTLS/SCTP and authorized
/// decoder. Only the former 8 ms polling gate versus Notify scheduling differs.
pub async fn input_benchmark(samples: usize, poll_8ms: bool) -> Result<InputEvidence> {
    let samples = samples.clamp(1, 5000);
    let (host, mut host_events, guest, _guest_events) = loopback_pair().await?;
    let result = async {
        let session = Uuid::new_v4();
        let feed = Arc::new(InputFeed::default());
        let mut sender = GuestInput::default();
        let mut grant = None;
        feed.publish(Some(Sample { id: 1, name: "HoriPlay synthetic benchmark only".into(), state: PadState::default(), captured: Instant::now() }));
        sender.flush(&guest, &feed, session, &mut grant).await?;
        // Synthetic local test permission; this does not create an Engine grant or OS device.
        grant = Some((1, 0));
        feed.arm(true);
        let mut receiver = Receiver::new(session, session);
        receiver.slots.assign(0, Owner { peer: session, controller: 1 }).map_err(anyhow::Error::msg)?;
        let (metadata_tx, mut metadata_rx) = tokio::sync::mpsc::channel(1);
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();
        let measuring = Arc::new(AtomicBool::new(false));
        let (source_feed, source_measuring) = (feed.clone(), measuring.clone());
        // Exercise a real cross-thread notification, as the SDL sampler does.
        // Acks isolate scheduling latency from intentional analog coalescing under load.
        let source = std::thread::spawn(move || -> Result<()> {
            let name: Arc<str> = "HoriPlay synthetic benchmark only".into();
            for n in 0..samples + 32 {
                std::thread::sleep(Duration::from_micros(1100 + (n as u64 * 7919) % 7000));
                if n == 32 { source_measuring.store(true, Ordering::Relaxed); }
                let begin = Instant::now();
                let state = InputSettings::default().apply(PadState { buttons: (n % 2) as u32, axes: [(n + 6000) as i16, -16000, 24000, -31000], triggers: [8192, 32767] });
                metadata_tx.blocking_send((n, state, begin))?;
                source_feed.publish(Some(Sample { id: 1, name: name.clone(), state, captured: begin }));
                ack_rx.recv_timeout(Duration::from_secs(2)).context("benchmark producer acknowledgement")?;
            }
            Ok(())
        });
        let send = async {
            let mut timer = tokio::time::interval(Duration::from_millis(if poll_8ms { 8 } else { 50 }));
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    _ = timer.tick() => {},
                    _ = feed.changed.notified(), if !poll_8ms => {},
                }
                sender.flush(&guest, &feed, session, &mut grant).await?;
                if !measuring.load(Ordering::Relaxed) { sender.wait = Default::default(); }
            }
            Ok::<_, anyhow::Error>(sender)
        };
        let receive = async move {
            let mut times = Vec::with_capacity(samples);
            let mut previous = PadState::default();
            for _ in 0..samples + 32 {
                let (n, state, begin) = metadata_rx.recv().await.context("benchmark producer ended")?;
                tokio::time::timeout(Duration::from_millis(250), async {
                loop {
                    let event = host_events.recv().await.context("benchmark peer closed")?;
                    if let PeerEvent::Input(bytes) = event {
                        let packet = InputPacket::decode(&bytes)?;
                        if let Ok(accepted) = receiver.accept(packet) {
                            // The previous sample can have a second (reliable/fast) copy or
                            // a fresh keepalive still in flight. It is not the new measurement.
                            if accepted == previous { continue; }
                            if accepted != state { bail!("benchmark delivered a different state: {accepted:?}, expected {state:?}"); }
                            break;
                        }
                    }
                }
                Ok::<_, anyhow::Error>(())
                }).await.context("synthetic input delivery timed out")??;
                if n >= 32 { times.push(begin.elapsed().as_secs_f64() * 1000.0); }
                previous = state;
                ack_tx.send(())?;
            }
            let _ = stop_tx.send(());
            Ok::<_, anyhow::Error>(times)
        };
        let measured = tokio::try_join!(send, receive);
        let source_result = source.join().map_err(|_| anyhow::anyhow!("benchmark producer panicked"))?;
        let (sender, times) = measured?;
        source_result?;
        Ok(InputEvidence {
            scheduling: if poll_8ms { "8ms polling gate (comparison fixture)" } else { "event notification (production)" }.into(),
            attempted: samples,
            accepted: times.len(),
            sample_to_authorized_receiver: percentiles(&times),
            wait_before_send: sender.wait.summary(),
            path: host.path().await,
            scope: "Same-machine synthetic sample -> production mailbox/sender -> real WebRTC DTLS/SCTP -> permission/sequence-checked receiver. 32 warmup samples excluded. Poll mode reproduces the former scheduling gate using the same optimized sender; it is not a run of the old full Engine. Excludes SDL physical acquisition, virtual driver, game, WAN, video/audio. Each sample must arrive; a timeout fails the command instead of disappearing from percentiles.".into(),
        })
    }.await;
    let _ = host.close().await;
    let _ = guest.close().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn production_event_sender_reaches_authorized_receiver() {
        let result = input_benchmark(20, false).await.unwrap();
        assert_eq!(result.accepted, 20);
        assert!(result.path.connected);
    }
}
