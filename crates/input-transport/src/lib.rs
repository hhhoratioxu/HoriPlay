use anyhow::{bail, Result};
use horiplay_core::{Owner, PadState, Slots};
use uuid::Uuid;

pub const PACKET_LEN: usize = 56;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputPacket {
    pub session: Uuid,
    pub controller: u32,
    pub slot: u8,
    pub sequence: u64,
    pub timestamp_us: u64,
    pub state: PadState,
}
impl InputPacket {
    pub fn encode(&self) -> [u8; PACKET_LEN] {
        let mut b = [0; PACKET_LEN];
        b[..3].copy_from_slice(b"HP\x01");
        b[3] = self.slot;
        b[4..20].copy_from_slice(self.session.as_bytes());
        b[20..24].copy_from_slice(&self.controller.to_le_bytes());
        b[24..32].copy_from_slice(&self.sequence.to_le_bytes());
        b[32..40].copy_from_slice(&self.timestamp_us.to_le_bytes());
        b[40..44].copy_from_slice(&self.state.buttons.to_le_bytes());
        for (i, v) in self.state.axes.iter().enumerate() {
            b[44 + i * 2..46 + i * 2].copy_from_slice(&v.to_le_bytes());
        }
        for (i, v) in self.state.triggers.iter().enumerate() {
            b[52 + i * 2..54 + i * 2].copy_from_slice(&v.to_le_bytes());
        }
        b
    }
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() != PACKET_LEN || &b[..3] != b"HP\x01" || b[3] > 3 {
            bail!("invalid input frame");
        }
        let v = Self {
            session: Uuid::from_slice(&b[4..20])?,
            controller: u32::from_le_bytes(b[20..24].try_into()?),
            slot: b[3],
            sequence: u64::from_le_bytes(b[24..32].try_into()?),
            timestamp_us: u64::from_le_bytes(b[32..40].try_into()?),
            state: PadState {
                buttons: u32::from_le_bytes(b[40..44].try_into()?),
                axes: std::array::from_fn(|i| i16::from_le_bytes([b[44 + i * 2], b[45 + i * 2]])),
                triggers: std::array::from_fn(|i| {
                    u16::from_le_bytes([b[52 + i * 2], b[53 + i * 2]])
                }),
            },
        };
        if !v.state.valid() {
            bail!("out of range input");
        }
        Ok(v)
    }
}

pub struct Receiver {
    session: Uuid,
    peer: Uuid,
    pub slots: Slots,
    last: [Option<u64>; 4],
}
impl Receiver {
    pub fn new(session: Uuid, peer: Uuid) -> Self {
        Self {
            session,
            peer,
            slots: Slots::default(),
            last: [None; 4],
        }
    }
    pub fn accept(&mut self, packet: InputPacket) -> Result<PadState> {
        if packet.session != self.session || !packet.state.valid() {
            bail!("session or state rejected");
        }
        let slot = packet.slot as usize;
        if slot >= 4 {
            bail!("invalid slot");
        }
        if !self.slots.permits(
            packet.slot,
            Owner {
                peer: self.peer,
                controller: packet.controller,
            },
        ) {
            bail!("controller not authorized");
        }
        if self.last[slot].is_some_and(|last| packet.sequence <= last) {
            bail!("stale or replayed input");
        }
        self.last[slot] = Some(packet.sequence);
        Ok(packet.state)
    }
    pub fn revoke(&mut self, controller: u32) {
        self.slots.revoke(Owner {
            peer: self.peer,
            controller,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet() -> InputPacket {
        InputPacket {
            session: Uuid::new_v4(),
            controller: 42,
            slot: 1,
            sequence: 1,
            timestamp_us: 23,
            state: PadState {
                buttons: 0b1011,
                axes: [-32768, 6553, 16384, 32767],
                triggers: [8192, 32767],
            },
        }
    }
    #[test]
    fn roundtrip_and_malformed() {
        let p = packet();
        assert_eq!(InputPacket::decode(&p.encode()).unwrap(), p);
        for len in 0..PACKET_LEN {
            assert!(InputPacket::decode(&p.encode()[..len]).is_err());
        }
        let mut b = p.encode();
        b[3] = 4;
        assert!(InputPacket::decode(&b).is_err());
        b = p.encode();
        b[55] = 255;
        assert!(InputPacket::decode(&b).is_err());
    }
    #[test]
    fn authority_replay_and_revoke() {
        let mut p = packet();
        let peer = Uuid::new_v4();
        let mut r = Receiver::new(p.session, peer);
        assert!(r.accept(p).is_err());
        r.slots
            .assign(
                1,
                Owner {
                    peer,
                    controller: 42,
                },
            )
            .unwrap();
        assert!(r.accept(p).is_ok());
        assert!(r.accept(p).is_err());
        p.sequence += 1;
        assert!(r.accept(p).is_ok());
        r.revoke(42);
        p.sequence += 1;
        assert!(r.accept(p).is_err());
    }
    #[test]
    fn reliable_edges_and_snapshots_share_sequence() {
        let mut p = packet();
        let peer = Uuid::new_v4();
        let mut r = Receiver::new(p.session, peer);
        r.slots
            .assign(
                1,
                Owner {
                    peer,
                    controller: 42,
                },
            )
            .unwrap();
        p.sequence = 5;
        r.accept(p).unwrap();
        p.sequence = 4;
        p.state.buttons = 1;
        assert!(r.accept(p).is_err());
        p.sequence = 6;
        p.state = PadState::default();
        assert!(r.accept(p).unwrap().neutral());
    }
}
