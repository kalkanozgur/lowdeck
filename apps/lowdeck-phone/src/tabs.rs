//! Süreçler ve AI raporu sekmelerinin metinleri (ADR-0009). Biçim lowmon-ui ile aynı (İngilizce).

use crate::{AiGroupRow, AiSessRow, ProcRow};
use lowdeck_proto::{AiReport, ProcItem};

pub fn proc_row(p: &ProcItem) -> ProcRow {
    ProcRow {
        pid: p.pid as i32,
        name: p.name.as_str().into(),
        // LM `fmt_cpu`: < %0,05 boş (liste sakin okunur)
        cpu: if p.cpu < 5 {
            String::new()
        } else {
            format!("{:.1}%", p.cpu as f32 / 100.0)
        }
        .into(),
        ram: format!("{} MB", p.ws_mb).into(),
        protected: p.protected,
    }
}

/// LM `procs::exit` kodları.
pub fn kill_text(code: i32, name: &str) -> String {
    match code {
        0 => format!("{name} ended"),
        2 => format!("{name}: process is gone (or the PID now belongs to another process)"),
        3 => format!("{name}: protected process, cannot be ended"),
        5 => format!("{name}: access denied (needs administrator; end it on the PC)"),
        c => format!("{name}: could not be ended (code {c})"),
    }
}

/// LM `report::format_tokens` (İngilizce): 812, 81.2 K, 3.04 M.
pub fn tokens(v: u64) -> String {
    let v = v as f64;
    let (n, unit) = match v {
        v if v < 1e3 => return format!("{v:.0}"),
        v if v < 1e6 => (v / 1e3, "K"),
        v => (v / 1e6, "M"),
    };
    let prec = if n < 10.0 { 2 } else { 1 };
    format!("{n:.prec$} {unit}")
}

/// 0.1 % → "47.7%".
fn permille(v: u16) -> String {
    format!("{:.1}%", v as f32 / 10.0)
}

pub fn sub(r: &AiReport) -> String {
    format!(
        "{} sessions · {} requests · total {}",
        r.session_count,
        r.request_count,
        tokens(r.total_tokens)
    )
}

pub fn groups(r: &AiReport) -> Vec<AiGroupRow> {
    let top = r.groups.first().map_or(1, |g| g.tokens.max(1)) as f32;
    // Yatay ekranda grafiğin altına 6 satır sığar (sunucu en çok 8 gönderir)
    r.groups
        .iter()
        .take(6)
        .map(|g| AiGroupRow {
            name: g.name.as_str().into(),
            pct: permille(g.share).into(),
            tokens: tokens(g.tokens).into(),
            fill: g.tokens as f32 / top,
        })
        .collect()
}

pub fn sessions(r: &AiReport, offset_s: i64) -> Vec<AiSessRow> {
    r.sessions
        .iter()
        .map(|s| AiSessRow {
            // "MM-DD HH:MM"
            end: if s.ended_at == 0 {
                "—".into()
            } else {
                crate::events::fmt_time(s.ended_at * 1000, offset_s)[..11].into()
            },
            project: if s.project.is_empty() {
                "—"
            } else {
                &s.project
            }
            .into(),
            requests: s.requests.to_string().into(),
            tokens: tokens(s.tokens).into(),
            top: if s.top.is_empty() {
                "—".into()
            } else {
                format!("{} {}", s.top, permille(s.top_share)).into()
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lowdeck_proto::{AiGroup, AiSession};

    #[test]
    fn texts() {
        assert_eq!(tokens(812), "812");
        assert_eq!(tokens(81_200), "81.2 K");
        assert_eq!(tokens(3_040_000), "3.04 M");
        assert_eq!(kill_text(0, "a.exe"), "a.exe ended");
        let p = proc_row(&ProcItem {
            pid: 7,
            cpu: 4,
            ws_mb: 12,
            name: "x".into(),
            ..Default::default()
        });
        assert_eq!((p.cpu.as_str(), p.ram.as_str()), ("", "12 MB"));
        let r = AiReport {
            session_count: 2,
            request_count: 9,
            total_tokens: 1500,
            groups: vec![
                AiGroup {
                    name: "Read".into(),
                    share: 477,
                    tokens: 1000,
                },
                AiGroup {
                    name: "Bash".into(),
                    share: 100,
                    tokens: 500,
                },
            ],
            sessions: vec![AiSession {
                ended_at: 0,
                top: "Read".into(),
                top_share: 477,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(sub(&r), "2 sessions · 9 requests · total 1.50 K");
        let g = groups(&r);
        assert_eq!((g[0].pct.as_str(), g[1].fill), ("47.7%", 0.5));
        let s = sessions(&r, 0);
        assert_eq!(
            (s[0].end.as_str(), s[0].project.as_str(), s[0].top.as_str()),
            ("—", "—", "Read 47.7%")
        );
    }
}
