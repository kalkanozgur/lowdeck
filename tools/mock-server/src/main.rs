//! Kullanım: `cargo run -p mock-server -- [port] [token]`
//! Varsayılan: 47810, token "dev".
//! Tek istemci, tek thread; gerçek ses/medya API'si çağırmaz.
//! Metrik aboneliğinde 2 s'de bir sahte `Metrics`, 6 s'de bir `Top` üretir.

use lowdeck_proto::*;
use std::io::{BufReader, BufWriter};
use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

const METRICS_EVERY: Duration = Duration::from_secs(2);

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let port: u16 = args.next().and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_PORT);
    let token = args.next().unwrap_or_else(|| "dev".into());

    let listener = TcpListener::bind(("0.0.0.0", port))?;
    println!("mock-server: 0.0.0.0:{port} token={token:?}");

    for conn in listener.incoming() {
        match conn {
            Ok(s) => {
                let peer = s.peer_addr().ok();
                println!("+ bağlandı: {peer:?}");
                if let Err(e) = handle(s, &token) {
                    println!("- koptu: {peer:?} ({e})");
                }
            }
            Err(e) => eprintln!("accept: {e}"),
        }
    }
    Ok(())
}

fn handle(stream: TcpStream, token: &str) -> Result<(), FrameError> {
    stream.set_nodelay(true).ok();
    let mut rd = BufReader::new(stream.try_clone()?);
    let mut wr = BufWriter::new(stream);
    let mut buf = [0u8; MAX_FRAME];

    let mut audio = AudioState { volume: 30, muted: false, device: "Mock Hoparlör".into() };
    let mut media = MediaState {
        has_session: true,
        playing: false,
        app: "Spotify".into(),
        title: "Örnek Parça".into(),
        artist: "Sanatçı".into(),
    };

    match read_frame::<_, C2S>(&mut rd, &mut buf)? {
        C2S::Hello { proto, token: t, name } => {
            if proto != PROTO_VERSION {
                write_frame(&mut wr, &S2C::Error(ErrorCode::BadVersion))?;
                return Ok(());
            }
            if t != token {
                write_frame(&mut wr, &S2C::Error(ErrorCode::BadToken))?;
                return Ok(());
            }
            println!("  hello: {name}");
            write_frame(&mut wr, &S2C::Welcome { proto: PROTO_VERSION, server: "mock".into() })?;
            // Bağlanınca tam durum gönderilir.
            write_frame(&mut wr, &S2C::Audio(audio.clone()))?;
            write_frame(&mut wr, &S2C::Media(media.clone()))?;
        }
        other => {
            println!("  Hello beklenirken: {other:?}");
            return Ok(());
        }
    }

    let mut tick: u32 = 0;
    loop {
        // shortcut: zaman aşımı çerçeve ortasına denk gelirse akış bozulur; localhost'ta çerçeve tek parça gelir.
        let msg: C2S = match read_frame(&mut rd, &mut buf) {
            Err(FrameError::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                tick = tick.wrapping_add(1);
                write_frame(&mut wr, &S2C::Metrics(Some(fake_metrics(tick))))?;
                if tick % 3 == 1 {
                    write_frame(&mut wr, &S2C::Top(fake_top(tick)))?;
                }
                continue;
            }
            r => r?,
        };
        println!("  <- {msg:?}");
        match msg {
            C2S::Ping(n) => write_frame(&mut wr, &S2C::Pong(n))?,
            C2S::Audio(cmd) => {
                match cmd {
                    AudioCmd::SetVolume(v) => audio.volume = v.min(100),
                    AudioCmd::Step(d) => {
                        audio.volume = (audio.volume as i16 + d as i16).clamp(0, 100) as u8
                    }
                    AudioCmd::ToggleMute => audio.muted = !audio.muted,
                }
                write_frame(&mut wr, &S2C::Audio(audio.clone()))?;
            }
            C2S::Media(cmd) => {
                match cmd {
                    MediaCmd::PlayPause => media.playing = !media.playing,
                    MediaCmd::Stop => media.playing = false,
                    MediaCmd::Next => media.title = format!("{} »", media.title),
                    MediaCmd::Prev => media.title = format!("« {}", media.title),
                }
                write_frame(&mut wr, &S2C::Media(media.clone()))?;
            }
            C2S::Subscribe(t) => {
                let on = t & TOPIC_METRICS != 0;
                rd.get_ref().set_read_timeout(on.then_some(METRICS_EVERY)).ok();
                if on {
                    tick = 0;
                    write_frame(&mut wr, &S2C::Metrics(Some(fake_metrics(0))))?;
                    write_frame(&mut wr, &S2C::Top(fake_top(0)))?;
                }
            }
            C2S::Hello { .. } => write_frame(&mut wr, &S2C::Error(ErrorCode::Unsupported))?,
        }
    }
}

/// Rastgele değil, tick'e bağlı dalga: ekranda değişim görünsün, test tekrarlanabilir olsun.
fn wave(t: u32, period: u32, max: u32) -> u32 {
    let p = t % period;
    max * if p < period / 2 { p } else { period - p } / (period / 2)
}

fn fake_metrics(t: u32) -> Metrics {
    Metrics {
        cpu: (500 + wave(t, 10, 6000)) as u16,
        cores: (0..8).map(|i| (wave(t + i * 2, 12, 190) + 5) as u8).collect(),
        ram_used_mb: 9000 + wave(t, 20, 4000),
        ram_total_mb: 32 * 1024,
        disk_r_kbps: wave(t, 8, 50_000),
        disk_w_kbps: wave(t + 3, 8, 8000),
        net_rx_kbps: wave(t, 6, 12_000),
        net_tx_kbps: wave(t + 2, 6, 900),
    }
}

fn fake_top(t: u32) -> Vec<TopItem> {
    let names = ["firefox.exe", "Code.exe", "cargo.exe", "explorer.exe", "lowmon-svc.exe"];
    names
        .iter()
        .enumerate()
        .map(|(i, n)| TopItem {
            name: (*n).into(),
            cpu: (wave(t + i as u32, 6, 1500) / (i as u32 + 1) + 10) as u16,
            ws_mb: 900 / (i as u32 + 1),
        })
        .collect()
}
