//! Komut satırı test istemcisi (UI'sız uçtan uca test).
//! `cargo run -p lowdeck-client --example cli -- 127.0.0.1 47810 dev`
//! Komutlar: v <0-100> | + | - | m | p | n | b | q

use lowdeck_client::{Client, Config};
use lowdeck_proto::{AudioCmd, MediaCmd, DEFAULT_PORT};
use std::io::BufRead;

fn main() {
    let mut a = std::env::args().skip(1);
    let host = a.next().unwrap_or_else(|| "127.0.0.1".into());
    let port = a.next().and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_PORT);
    let token = a.next().unwrap_or_else(|| "dev".into());

    let c = Client::start(Config { host, port, token, name: "cli".into() }, |ev| println!("<< {ev:?}"));

    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let mut it = line.split_whitespace();
        let ok = match it.next() {
            Some("v") => c.audio(AudioCmd::SetVolume(it.next().and_then(|s| s.parse().ok()).unwrap_or(50))),
            Some("+") => c.audio(AudioCmd::Step(2)),
            Some("-") => c.audio(AudioCmd::Step(-2)),
            Some("m") => c.audio(AudioCmd::ToggleMute),
            Some("p") => c.media(MediaCmd::PlayPause),
            Some("n") => c.media(MediaCmd::Next),
            Some("b") => c.media(MediaCmd::Prev),
            Some("q") => break,
            _ => continue,
        };
        if !ok {
            println!("!! bağlı değil");
        }
    }
}
