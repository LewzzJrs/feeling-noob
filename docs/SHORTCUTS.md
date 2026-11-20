# SHORTCUTS

Catatan jalan pintas di kode (tech debt). Bukan daftar shortcut keyboard; itu ada di `SPEC.md` bagian 6.

Format tiap entri: file, apa yang di-skip, kapan perlu diperbaiki.

## Fase 2

| # | File | Yang di-skip | Kapan diperbaiki |
|---|---|---|---|
| S1 | `crates/fnoob-engine/src/device.rs` | Hanya format sampel `f32`. Device dengan format lain ditolak dengan pesan error. WASAPI mode shared hampir selalu `f32`. | Kalau ada device yang ditolak. |
| S2 | `crates/fnoob-engine/src/device.rs` | Selalu pakai konfigurasi default device (sample rate, channel, buffer size). Buffer size belum bisa dipilih. | Fase 5, saat latensi rekaman diukur. |
| S3 | `crates/fnoob-engine/src/device.rs` | Error stream hanya menyimpan `ErrorKind`, pesan detail hilang. Tidak ada reconnect otomatis; user menekan "Reopen device" di Settings. | Fase 8, atau lebih awal kalau sering terjadi. |
| S4 | `crates/fnoob-app/src/app.rs` | Tempo, metronome, dan birama 4/4 disimpan di struct `App`, bukan di model project. | Fase 3 (model `Project`). |
| S5 | `crates/fnoob-app/src/app.rs` | Belum ada keymap; transport hanya lewat tombol. LCD hanya bar.beat. | Fase 3. |
| S6 | `crates/fnoob-app/src/main.rs` | Ikon jendela masih ikon default `eframe`. | Fase 8 (ikon original). |
| S7 | `crates/fnoob-core/src/fsutil.rs`, `settings.rs` (tes) | Tes memakai folder di `%TEMP%` dengan nama berbasis PID, bukan crate `tempfile`. | Kalau tes mulai bentrok saat jalan paralel. |
