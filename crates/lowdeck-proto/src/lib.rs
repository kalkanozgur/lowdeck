//! LowDeck kablo protokolü.
//!
//! - Taşıma: TCP, tek kalıcı bağlantı (istemci = telefon, sunucu = LowMonitor modülü).
//! - Çerçeve: `[u16 LE uzunluk][postcard payload]`, payload <= `MAX_FRAME`.
//! - Bu crate LowMonitor tarafında da AYNEN kullanılır; tipler tek kaynaktan gelir.
//!
//! Uyumluluk kuralı: enum varyantları yalnızca SONA eklenir (postcard varyant
//! indeksini kodlar). Kırıcı değişiklikte `PROTO_VERSION` artırılır.

use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

pub const PROTO_VERSION: u16 = 1;
pub const DEFAULT_PORT: u16 = 47800;
pub const MAX_FRAME: usize = 4096;

// ---------------- İstemci -> Sunucu ----------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum C2S {
    /// Bağlantıdan sonraki ilk mesaj. Token LAN içi basit paylaşılan anahtar.
    Hello { proto: u16, token: String, name: String },
    /// Ölü bağlantı tespiti; sunucu aynı sayıyla Pong döner.
    Ping(u32),
    Audio(AudioCmd),
    Media(MediaCmd),
    // İleride (SONA eklenecek): Lock(LockCmd), Subscribe(Topics) ...
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum AudioCmd {
    /// Mutlak seviye 0..=100 (sunucu clamp eder).
    SetVolume(u8),
    /// Göreli adım, ör. +2 / -2.
    Step(i8),
    ToggleMute,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum MediaCmd {
    PlayPause,
    Next,
    Prev,
    Stop,
}

// ---------------- Sunucu -> İstemci ----------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum S2C {
    Welcome { proto: u16, server: String },
    Pong(u32),
    /// Durum değiştiğinde PUSH edilir (istemci polling yapmaz).
    Audio(AudioState),
    Media(MediaState),
    Error(ErrorCode),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct AudioState {
    pub volume: u8,
    pub muted: bool,
    pub device: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MediaState {
    pub has_session: bool,
    pub playing: bool,
    pub app: String,
    pub title: String,
    pub artist: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum ErrorCode {
    BadVersion,
    BadToken,
    Unsupported,
    Internal,
}

// ---------------- Çerçeveleme ----------------

#[derive(Debug)]
pub enum FrameError {
    Io(io::Error),
    TooLarge(usize),
    Decode(postcard::Error),
}

impl From<io::Error> for FrameError {
    fn from(e: io::Error) -> Self {
        FrameError::Io(e)
    }
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::Io(e) => write!(f, "io: {e}"),
            FrameError::TooLarge(n) => write!(f, "frame too large: {n}"),
            FrameError::Decode(e) => write!(f, "decode: {e}"),
        }
    }
}

impl std::error::Error for FrameError {}

/// Tek çağrıda tek `write_all`: Nagle ile birlikte küçük paket sayısını azaltır.
pub fn write_frame<W: Write, T: Serialize>(w: &mut W, msg: &T) -> Result<(), FrameError> {
    let mut buf = [0u8; MAX_FRAME + 2];
    let n = postcard::to_slice(msg, &mut buf[2..])
        .map_err(FrameError::Decode)?
        .len();
    buf[..2].copy_from_slice(&(n as u16).to_le_bytes());
    w.write_all(&buf[..n + 2])?;
    w.flush()?;
    Ok(())
}

/// `buf` çağıran tarafından tekrar kullanılır: okuma döngüsünde alloc yok
/// (String alanlar hariç).
pub fn read_frame<R: Read, T: for<'de> Deserialize<'de>>(
    r: &mut R,
    buf: &mut [u8; MAX_FRAME],
) -> Result<T, FrameError> {
    let mut len = [0u8; 2];
    r.read_exact(&mut len)?;
    let n = u16::from_le_bytes(len) as usize;
    if n > MAX_FRAME {
        return Err(FrameError::TooLarge(n));
    }
    r.read_exact(&mut buf[..n])?;
    postcard::from_bytes(&buf[..n]).map_err(FrameError::Decode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let msgs = [
            C2S::Hello { proto: PROTO_VERSION, token: "t".into(), name: "toco".into() },
            C2S::Audio(AudioCmd::SetVolume(42)),
            C2S::Audio(AudioCmd::Step(-2)),
            C2S::Media(MediaCmd::Next),
            C2S::Ping(7),
        ];
        let mut wire = Vec::new();
        for m in &msgs {
            write_frame(&mut wire, m).unwrap();
        }
        let mut rd = &wire[..];
        let mut buf = [0u8; MAX_FRAME];
        for m in &msgs {
            let got: C2S = read_frame(&mut rd, &mut buf).unwrap();
            assert_eq!(&got, m);
        }
    }

    #[test]
    fn small_on_wire() {
        let mut wire = Vec::new();
        write_frame(&mut wire, &C2S::Media(MediaCmd::PlayPause)).unwrap();
        assert!(wire.len() <= 4, "komut çerçevesi {} bayt", wire.len());
    }
}
