# ADR-0007: PC keşfi: UDP sorgu–yanıt (beacon yayını yok)
- Tarih: 2026-11-15
- Durum: Kabul (PROTOCOL.md §5 taslağının yerine geçer)

## Bağlam
Telefon ayarında PC IP'si elle giriliyor. Taslak (§5): agent 5 s'de bir UDP beacon yayınlar, istemci ayar ekranında dinler. Bu, agent'ta ve LAN'daki her cihazda sürekli uyanma demek (5 s × günde 17 280 paket); CLAUDE.md §5 "periyodik iş yok" kuralıyla çelişir.

## Karar
1. **Sorgu–yanıt:** istemci yalnız "PC'yi bul"a basılınca `255.255.255.255:47811`'e 4 bayt `LDQ1` gönderir (600 ms sonra bir kez tekrar, Wi-Fi kaybı), 1,5 s yanıt toplar. Agent tek datagram ile yanıtlar: `LDA1` + postcard `Beacon { proto, host, port, name }`.
2. **Token yanıtta yok.** Keşif yalnız adres verir; token yine agent tray menüsünden elle girilir (LAN'daki herkes yanıtı görebilir).
3. **Agent:** `0.0.0.0:47811` (yayın almak için tüm arayüzler), yanıttaki `host` = `agent.cfg` `bind`. Yalnız `LDQ1` yanıtlanır, yanıt arası ≥ 100 ms, metin alanları ≤ 63 karakter (yanıt ~40–90 B, yansıtma kazancı sınırlı, yalnız LAN + firewall `localsubnet`). Bind loopback ise keşif kapalı. Sorgu gelmezse thread `recv_from`'da bloklu: uyanma yok.
4. **Firewall:** "Firewall kuralı ekle" artık TCP port + UDP 47811 (aynı kural adı, Private + yerel alt ağ, bu exe).
5. TCP protokolü değişmedi (`PROTO_VERSION` 4); keşif ayrı kanal, sürüm yanıtta bilgi olarak ("sürüm farklı" uyarısı).

## Sonuçlar
- \+ Boşta sıfır trafik/uyanma; telefonda IP yazmak gerekmiyor.
- − Yayın paketleri bazı ağlarda (misafir Wi-Fi, AP izolasyonu, farklı VLAN) geçmez → elle IP yine mümkün.
- − Var olan firewall kuralı UDP'yi içermez: kullanıcı "Firewall kuralı ekle"yi bir kez daha çalıştırmalı.
