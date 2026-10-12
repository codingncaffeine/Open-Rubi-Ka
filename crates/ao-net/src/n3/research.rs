//! Native global research and vehicle requests (Gamecode.dll).
//! ResearchUpdate vtable 10156078: reader 1003a58d, writer 1003a679.
use anyhow::{bail, Result};
use crate::{msg::Identity, Reader, Writer};

pub const UPDATE: u32 = 0x253D0240;
pub const SPAWN_MECH: u32 = 0x464D000A;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Goal {
    pub id: i32,
    /// Remaining research for native side indices 0, 1, 2; -1 means unavailable.
    pub remaining: [i32; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    /// Native reader consumes but does not branch on this byte; writer emits 1.
    pub version: u8,
    pub goals: Vec<Goal>,
}

pub fn parse_update(body: &[u8]) -> Result<Update> {
    let mut r = Reader::new(body);
    let version = r.u8()?;
    let mut goals = Vec::new();
    loop {
        let id = r.i32()?;
        if id == 0 {
            if r.remaining() != 0 { bail!("ResearchUpdate trailing bytes"); }
            return Ok(Update { version, goals });
        }
        goals.push(Goal { id, remaining: [r.i32()?, r.i32()?, r.i32()?] });
    }
}

fn request_header(key: u32, char_id: i32) -> Writer {
    let mut w = Writer::default();
    w.u32(key);
    Identity { kind: super::outgoing::DYNEL_CHAR, instance: char_id }.write(&mut w);
    w.u8(0);
    w
}

/// ResearchRequest ctor 1002cd6a clears pass-on; serializer 100793e8 writes nothing.
pub fn request(char_id: i32) -> Vec<u8> {
    request_header(super::outgoing::message_key("ResearchRequestIIR"), char_id).0
}

/// SpawnMech ctor 1007980e clears pass-on; serializer 100796f4 writes one i32.
pub fn spawn_mech(char_id: i32, vehicle: i32) -> Vec<u8> {
    let mut w = request_header(SPAWN_MECH, char_id);
    w.i32(vehicle);
    w.0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_field_sequence_and_truncations() {
        // Literal native serializer sequence: version, id, three side timers, sentinel.
        // RE-derived bytes, not a claimed server capture.
        let bytes = [1, 0,0,0,7, 0,0,0,9, 0,0,0,0, 255,255,255,255, 0,0,0,0];
        assert_eq!(parse_update(&bytes).unwrap(), Update { version: 1, goals: vec![Goal { id: 7, remaining: [9,0,-1] }] });
        for n in 0..bytes.len() { assert!(parse_update(&bytes[..n]).is_err()); }
        let mut extra = bytes.to_vec(); extra.push(0);
        assert!(parse_update(&extra).is_err());
        let mech = spawn_mech(42, 7);
        assert_eq!(mech, [0x46,0x4d,0,0x0a, 0,0,0xc3,0x50, 0,0,0,42, 0, 0,0,0,7]);
        let req = request(42);
        assert_eq!(req.len(), 13);
        assert_eq!(&req[4..], &[0,0,0xc3,0x50,0,0,0,42,0]);
    }
}
