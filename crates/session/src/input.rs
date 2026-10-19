use super::{control, Control};
use anyhow::{bail, Result};
use horiplay_core::{PadState, TimingWindow};
use horiplay_input_transport::InputPacket;
use horiplay_networking::Peer;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::Notify;
use uuid::Uuid;

const MAX_EDGES: usize = 64;
const KEEPALIVE: Duration = Duration::from_millis(50);
const MAX_SAMPLE_AGE: Duration = Duration::from_millis(100);

#[derive(Clone, Debug)]
pub(crate) struct Sample {
    pub id: u32,
    pub name: Arc<str>,
    pub state: PadState,
    pub captured: Instant,
}
#[derive(Default)]
struct Pending {
    latest: Option<Sample>,
    edges: VecDeque<Option<Sample>>,
    armed: bool,
    overflow: bool,
}
/// Single producer/consumer: analog samples replace the latest value; button/lifecycle
/// transitions have a bounded FIFO. A delayed consumer never accumulates analog work.
#[derive(Default)]
pub(crate) struct InputFeed {
    pending: Mutex<Pending>,
    pub changed: Notify,
}
impl InputFeed {
    pub fn publish(&self, sample: Option<Sample>) {
        let mut pending = self.pending.lock().unwrap();
        let key = |value: &Option<Sample>| value.as_ref().map(|s| (s.id, s.state));
        let changed = key(&pending.latest) != key(&sample);
        let buttons = |value: &Option<Sample>| value.as_ref().map(|s| (s.id, s.state.buttons));
        if pending.armed && buttons(&pending.latest) != buttons(&sample) {
            if pending.edges.len() == MAX_EDGES {
                // Fail the session rather than silently losing a release or growing a queue.
                pending.overflow = true;
            } else if !pending.overflow {
                pending.edges.push_back(sample.clone());
            }
        }
        pending.latest = sample;
        drop(pending);
        if changed {
            self.changed.notify_one();
        }
    }
    pub fn arm(&self, armed: bool) {
        let mut pending = self.pending.lock().unwrap();
        pending.edges.clear();
        pending.overflow = false;
        pending.armed = armed;
        drop(pending);
        self.changed.notify_one();
    }
    fn take(&self) -> Result<(Vec<Option<Sample>>, Option<Sample>)> {
        let mut pending = self.pending.lock().unwrap();
        if pending.overflow {
            bail!("input edge buffer overflow; stopping to avoid losing a button release");
        }
        // Empty analog-only batches allocate no edge storage. Keep producer capacity reusable.
        let edges = pending.edges.drain(..).collect();
        Ok((edges, pending.latest.clone()))
    }
}

pub(crate) struct GuestInput {
    pub announced: Option<u32>,
    pub retry_pending: bool,
    last_buttons: Option<u32>,
    last_sent: Option<(PadState, Instant)>,
    sequence: u64,
    start: Instant,
    pub serialization: TimingWindow<1024>,
    pub wait: TimingWindow<1024>,
}
impl Default for GuestInput {
    fn default() -> Self {
        Self {
            announced: None,
            retry_pending: false,
            last_buttons: None,
            last_sent: None,
            sequence: 0,
            start: Instant::now(),
            serialization: TimingWindow::default(),
            wait: TimingWindow::default(),
        }
    }
}
impl GuestInput {
    pub fn new_grant(&mut self) {
        self.last_buttons = None;
        self.last_sent = None;
        self.retry_pending = false;
    }
    async fn announce(
        &mut self,
        peer: &Peer,
        sample: Option<&Sample>,
        grant: &mut Option<(u32, u8)>,
        feed: &InputFeed,
    ) -> Result<()> {
        let selected = sample.map(|s| s.id);
        if self.announced != selected {
            if let Some(id) = self.announced {
                control(peer, &Control::Disconnect { id }).await?;
            }
            *grant = None;
            feed.arm(false);
            self.new_grant();
            if let Some(sample) = sample {
                control(
                    peer,
                    &Control::Connect {
                        id: sample.id,
                        name: sample.name.to_string(),
                    },
                )
                .await?;
            }
            self.announced = selected;
        }
        Ok(())
    }
    async fn send(
        &mut self,
        peer: &Peer,
        session: Uuid,
        sample: &Sample,
        slot: u8,
        fast: bool,
    ) -> Result<u64> {
        if sample.captured.elapsed() > MAX_SAMPLE_AGE {
            bail!("input sampler stalled; refusing stale input keepalive");
        }
        let edge = self.last_buttons != Some(sample.state.buttons);
        if !edge
            && (!fast
                || self
                    .last_sent
                    .is_some_and(|(state, at)| state == sample.state && at.elapsed() < KEEPALIVE))
        {
            if fast {
                self.retry_pending = false;
            }
            return Ok(0);
        }
        let begin = Instant::now();
        self.sequence += 1;
        let bytes = InputPacket {
            session,
            controller: sample.id,
            slot,
            sequence: self.sequence,
            timestamp_us: self.start.elapsed().as_micros() as u64,
            state: sample.state,
        }
        .encode();
        self.serialization
            .record(begin.elapsed().as_secs_f64() * 1000.0);
        self.wait
            .record(sample.captured.elapsed().as_secs_f64() * 1000.0);
        if edge {
            if !peer.send(&bytes, true).await? {
                bail!("button edge could not be sent");
            }
            self.last_buttons = Some(sample.state.buttons);
        }
        let sent = fast && peer.send(&bytes, false).await?;
        if fast {
            self.retry_pending = !sent;
        }
        if sent {
            self.last_sent = Some((sample.state, Instant::now()));
        }
        Ok(u64::from(sent))
    }
    pub async fn flush(
        &mut self,
        peer: &Peer,
        feed: &InputFeed,
        session: Uuid,
        grant: &mut Option<(u32, u8)>,
    ) -> Result<u64> {
        let (edges, latest) = feed.take()?;
        if !peer.ready().await {
            self.retry_pending = false;
            return Ok(0);
        }
        // Preserve captured press/release order before sending the newest analog snapshot.
        for edge in edges {
            self.announce(peer, edge.as_ref(), grant, feed).await?;
            if let (Some(sample), Some((id, slot))) = (edge.as_ref(), *grant) {
                if sample.id == id {
                    self.send(peer, session, sample, slot, false).await?;
                }
            }
        }
        self.announce(peer, latest.as_ref(), grant, feed).await?;
        if let (Some(sample), Some((id, slot))) = (latest.as_ref(), *grant) {
            if sample.id == id {
                return self.send(peer, session, sample, slot, true).await;
            }
        }
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(buttons: u32, axis: i16) -> Sample {
        Sample {
            id: 1,
            name: "synthetic test only".into(),
            state: PadState {
                buttons,
                axes: [axis, 0, 0, 0],
                ..Default::default()
            },
            captured: Instant::now(),
        }
    }
    #[test]
    fn analog_coalesces_but_short_taps_and_disconnect_survive() {
        let feed = InputFeed::default();
        feed.publish(Some(sample(0, 0)));
        feed.arm(true);
        for axis in 1..1000 {
            feed.publish(Some(sample(0, axis)));
        }
        feed.publish(Some(sample(1, 1000)));
        feed.publish(Some(sample(0, 2000)));
        let (edges, latest) = feed.take().unwrap();
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].as_ref().unwrap().state.buttons, 1);
        assert_eq!(edges[1].as_ref().unwrap().state.buttons, 0);
        assert_eq!(latest.unwrap().state.axes[0], 2000);
        feed.publish(None);
        assert!(feed.take().unwrap().0[0].is_none());
    }
    #[test]
    fn overflow_is_fatal_and_ungranted_input_does_not_accumulate() {
        let feed = InputFeed::default();
        for n in 0..1000 {
            feed.publish(Some(sample(n % 2, 0)));
        }
        assert!(feed.take().unwrap().0.is_empty());
        feed.arm(true);
        for n in 0..=MAX_EDGES {
            feed.publish(Some(sample((n % 2) as u32, 0)));
        }
        assert!(feed.take().is_err());
        feed.arm(false);
        assert!(feed.take().is_ok());
    }
    #[tokio::test]
    async fn notification_survives_publish_before_wait() {
        let feed = InputFeed::default();
        feed.publish(Some(sample(1, 0)));
        tokio::time::timeout(Duration::from_millis(100), feed.changed.notified())
            .await
            .unwrap();
        assert_eq!(feed.take().unwrap().1.unwrap().state.buttons, 1);
    }
    #[tokio::test]
    async fn a_stalled_sampler_cannot_refresh_remote_input() {
        let (host, _host_events, guest, _guest_events) =
            horiplay_networking::loopback_pair().await.unwrap();
        let feed = InputFeed::default();
        feed.publish(Some(sample(0, 0)));
        let mut sender = GuestInput::default();
        let mut grant = None;
        let session = Uuid::new_v4();
        sender
            .flush(&guest, &feed, session, &mut grant)
            .await
            .unwrap();
        grant = Some((1, 0));
        feed.arm(true);
        let mut stale = sample(1, 3000);
        stale.captured = Instant::now() - MAX_SAMPLE_AGE - Duration::from_millis(1);
        feed.publish(Some(stale));
        let error = sender
            .flush(&guest, &feed, session, &mut grant)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("sampler stalled"));
        assert_eq!(sender.sequence, 0, "no stale report was encoded or sent");
        host.close().await.unwrap();
        guest.close().await.unwrap();
    }
}
