// SPDX-License-Identifier: AGPL-3.0-or-later
//! CTAPHID framing (CTAP 2.1, §11.2) on top of a raw HID report transport.

use std::time::{Duration, Instant};

use rand_core::{OsRng, RngCore};

use crate::error::FidoError;
use crate::{FidoListener, FidoStatus, HidTransport};

pub const BROADCAST_CID: u32 = 0xffff_ffff;
const CMD_INIT: u8 = 0x06;
const CMD_CBOR: u8 = 0x10;
const CMD_CANCEL: u8 = 0x11;
const CMD_KEEPALIVE: u8 = 0x3b;
const CMD_ERROR: u8 = 0x3f;

const KEEPALIVE_PROCESSING: u8 = 1;
const KEEPALIVE_UPNEEDED: u8 = 2;

/// Per-read timeout. Authenticators send keepalives about every 100 ms while busy.
const READ_TIMEOUT_MS: u32 = 1_000;

pub struct HidInfo {
    pub protocol_version: u8,
    pub device_version: (u8, u8, u8),
    pub capabilities: u8,
}

pub struct CtapHid<'a> {
    transport: &'a dyn HidTransport,
    listener: Option<&'a dyn FidoListener>,
    report_size: usize,
    cid: u32,
    pub info: HidInfo,
}

impl<'a> CtapHid<'a> {
    /// Opens a channel with CTAPHID_INIT.
    pub fn open(transport: &'a dyn HidTransport, listener: Option<&'a dyn FidoListener>) -> Result<Self, FidoError> {
        let report_size = transport.report_size() as usize;
        if !(16..=64).contains(&report_size) {
            return Err(FidoError::protocol(format!("unsupported HID report size {report_size}")));
        }
        let mut hid = CtapHid {
            transport,
            listener,
            report_size,
            cid: BROADCAST_CID,
            info: HidInfo { protocol_version: 0, device_version: (0, 0, 0), capabilities: 0 },
        };
        let mut nonce = [0u8; 8];
        OsRng.fill_bytes(&mut nonce);
        hid.send(BROADCAST_CID, CMD_INIT, &nonce)?;
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let resp = hid.receive(BROADCAST_CID, deadline)?;
            if resp.0 != CMD_INIT || resp.1.len() < 17 || resp.1[..8] != nonce {
                continue; // response to someone else's INIT
            }
            let d = resp.1;
            hid.cid = u32::from_be_bytes([d[8], d[9], d[10], d[11]]);
            hid.info = HidInfo { protocol_version: d[12], device_version: (d[13], d[14], d[15]), capabilities: d[16] };
            return Ok(hid);
        }
    }

    /// Sends a CTAP2 CBOR command and returns the response (status byte + payload).
    /// `timeout` should cover user interaction for commands that require a touch.
    pub fn cbor(&self, payload: &[u8], timeout: Duration) -> Result<Vec<u8>, FidoError> {
        self.send(self.cid, CMD_CBOR, payload)?;
        let deadline = Instant::now() + timeout;
        match self.receive(self.cid, deadline) {
            Ok((CMD_CBOR, data)) => Ok(data),
            Ok((cmd, _)) => Err(FidoError::protocol(format!("unexpected CTAPHID command 0x{cmd:02x}"))),
            Err(FidoError::Timeout) => {
                let _ = self.send(self.cid, CMD_CANCEL, &[]);
                Err(FidoError::Timeout)
            }
            Err(e) => Err(e),
        }
    }

    fn send(&self, cid: u32, cmd: u8, data: &[u8]) -> Result<(), FidoError> {
        if data.len() > u16::MAX as usize {
            return Err(FidoError::protocol("message too large"));
        }
        let size = self.report_size;
        let mut packet = vec![0u8; size];
        packet[..4].copy_from_slice(&cid.to_be_bytes());
        packet[4] = 0x80 | cmd;
        packet[5..7].copy_from_slice(&(data.len() as u16).to_be_bytes());
        let first = data.len().min(size - 7);
        packet[7..7 + first].copy_from_slice(&data[..first]);
        self.transport.write_report(packet)?;

        let mut offset = first;
        let mut seq: u8 = 0;
        while offset < data.len() {
            if seq > 0x7f {
                return Err(FidoError::protocol("message too large for CTAPHID"));
            }
            let mut packet = vec![0u8; size];
            packet[..4].copy_from_slice(&cid.to_be_bytes());
            packet[4] = seq;
            let n = (data.len() - offset).min(size - 5);
            packet[5..5 + n].copy_from_slice(&data[offset..offset + n]);
            self.transport.write_report(packet)?;
            offset += n;
            seq += 1;
        }
        Ok(())
    }

    /// Receives one complete message on `cid`, handling keepalives and errors.
    fn receive(&self, cid: u32, deadline: Instant) -> Result<(u8, Vec<u8>), FidoError> {
        loop {
            let first = self.read_packet(cid, deadline)?;
            if first[4] & 0x80 == 0 {
                continue; // stray continuation packet
            }
            let cmd = first[4] & 0x7f;
            let total = u16::from_be_bytes([first[5], first[6]]) as usize;
            let mut data = Vec::with_capacity(total);
            data.extend_from_slice(&first[7..(7 + total).min(first.len())]);
            let mut seq: u8 = 0;
            while data.len() < total {
                let cont = self.read_packet(cid, deadline)?;
                if cont[4] & 0x80 != 0 || cont[4] != seq {
                    return Err(FidoError::protocol("CTAPHID sequence error"));
                }
                let n = (total - data.len()).min(cont.len() - 5);
                data.extend_from_slice(&cont[5..5 + n]);
                seq = seq.wrapping_add(1);
            }
            match cmd {
                CMD_KEEPALIVE => {
                    if let Some(l) = self.listener {
                        match data.first() {
                            Some(&KEEPALIVE_UPNEEDED) => l.on_status(FidoStatus::TouchNeeded),
                            Some(&KEEPALIVE_PROCESSING) => l.on_status(FidoStatus::Processing),
                            _ => {}
                        }
                    }
                    continue;
                }
                CMD_ERROR => return Err(FidoError::Hid { code: data.first().copied().unwrap_or(0x7f) }),
                _ => return Ok((cmd, data)),
            }
        }
    }

    fn read_packet(&self, cid: u32, deadline: Instant) -> Result<Vec<u8>, FidoError> {
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(FidoError::Timeout);
            }
            let remaining = (deadline - now).as_millis().min(READ_TIMEOUT_MS as u128) as u32;
            match self.transport.read_report(remaining.max(1))? {
                None => continue,
                Some(p) if p.len() < 7 => {
                    return Err(FidoError::protocol(format!("short HID report ({} bytes)", p.len())))
                }
                Some(p) if u32::from_be_bytes([p[0], p[1], p[2], p[3]]) == cid => return Ok(p),
                Some(_) => continue, // other channel
            }
        }
    }
}
