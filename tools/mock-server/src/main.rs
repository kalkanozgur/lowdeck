//! Kullanım: `cargo run -p mock-server -- [port] [token]`
//! Varsayılan: 47810, token "dev".
//! Tek istemci, tek thread; gerçek ses/medya API'si çağırmaz.

use lowdeck_proto::*;
use std::io::{BufReader, BufWriter};
use std::net::{TcpListener, TcpStream};

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

    loop {
        let msg: C2S = read_frame(&mut rd, &mut buf)?;
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
            C2S::Hello { .. } => write_frame(&mut wr, &S2C::Error(ErrorCode::Unsupported))?,
        }
    }
}
