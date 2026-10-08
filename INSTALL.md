# Panduan Instalasi authFace

Panduan lengkap untuk **instal di laptop baru**, **distro hop**, atau **install ulang**
supaya tidak perlu ngoprek lagi. Semua perintah bisa langsung copy-paste.

> Dokumen ini dalam Bahasa Indonesia. Dokumen teknis lain (README, CHANGELOG) dalam
> Bahasa Inggris.

---

## Daftar Isi

1. [TL;DR — 5 menit](#1-tl-dr--5-menit)
2. [Cek dulu: laptop ini layak?](#2-cek-dulu-laptop-ini-layak)
3. [Pilih jalur instalasi](#3-pilih-jalur-instalasi)
4. [Instalasi langkah demi langkah](#4-instalasi-langkah-demi-langkah)
5. [Enroll wajah & verifikasi](#5-enroll-wajah--verifikasi)
6. [Komponen opsional](#6-komponen-opsional)
7. [Install ulang / distro hop (backup & restore)](#7-install-ulang--distro-hop-backup--restore)
8. [Bawa ke laptop lain / install offline](#8-bawa-ke-laptop-lain--install-offline)
9. [Troubleshooting](#9-troubleshooting)
10. [Uninstall](#10-uninstall)
11. [Checklist cepat](#11-checklist-cepat)

---

## 1. TL;DR — 5 menit

Kalau sudah pernah install sebelumnya dan cuma pindah mesin:

```bash
git clone https://github.com/SamVivan1/authFace.git
cd authFace

# (sekali) build toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add x86_64-unknown-linux-musl

cargo build --release --target x86_64-unknown-linux-musl -p face-auth -p face-enroll

sudo ./deploy.sh          # binary + model + PAM + folder embeddings
./deploy-gui.sh           # (opsional) panel settings GUI

face-enroll --user $USER -f 8 -v     # enroll wajah, duduk diam di depan kamera
sudo true                             # tes → exit 0 = sukses
```

Selesai. Password tetap jadi cadangan, jadi **tidak mungkin terkunci**.

---

## 2. Cek dulu: laptop ini layak?

### a. Ada kamera IR?

```bash
for d in /sys/class/video4linux/*; do echo "$(basename $d): $(cat $d/name)"; done
```

Contoh output di mesin ini:

```
video0: HP 5MP Camera: HP 5MP Camera      ← RGB (biasa)
video1: HP 5MP Camera: HP 5MP Camera      ← RGB (biasa)
video2: HP 5MP Camera: HP IR Camera       ← IR (yang dipakai)
video3: HP 5MP Camera: HP IR Camera       ← IR (node metadata, JANGAN dipakai)
```

Yang dicari: nama mengandung **IR** / **infrared** (Windows Hello camera).
Kalau tidak ada → authFace tidak bisa dipakai (kamera RGB biasa tidak punya
format GREY mentah yang dibutuhkan).

### b. User masuk grup `video`

```bash
id -nG | grep -q video || sudo usermod -aG video $USER
# logout/login setelah itu
```

### c. Persyaratan software

| Komponen | Syarat | Cek |
|---|---|---|
| PAM + `pam_exec.so` | bawaan semua distro | `find /usr/lib* /lib* -name pam_exec.so 2>/dev/null` |
| Distro | Fedora/Bluefin/Bazzite/Silverblue, Ubuntu/Debian, Arch, openSUSE | — |
| GNOME Shell (opsional, untuk indikator lock screen) | 45+ | `gnome-shell --version` |
| GTK4 + libadwaita (opsional, untuk GUI) | GTK ≥ 4.14, libadwaita ≥ 1.5 | `pkg-config --modversion gtk4 libadwaita-1` |
| Arsitektur | x86_64 saja | `uname -m` |

> **Tidak perlu**: systemd service, daemon, D-Bus server, layering rpm-ostree,
> atau package sistem apapun. Semua muat di `/usr/local` + `~/.local`.

---

## 3. Pilih jalur instalasi

| Jalur | Kapan pakai | Butuh Rust? | Butuh internet? |
|---|---|---|---|
| **A. Binary jadi (paling gampang)** | Punya binary hasil build dari mesin lain | ❌ | sekali (download model) |
| **B. Build di laptop tujuan** | Laptop baru, mau versi terbaru | ✅ | ✅ |
| **C. Build via distrobox/container** | Distro immutable (Silverblue, Bazzite, Bluefin) yang susah install toolchain | ✅ (di container) | ✅ |

### Jalur A — binary sudah jadi (tanpa Rust)

Binary `face-auth` dan `face-enroll` di-build **statis (musl)** → jalan di distro
manapun tanpa dependensi. Dari mesin lama, bawa:

```
target/x86_64-unknown-linux-musl/release/face-auth
target/x86_64-unknown-linux-musl/release/face-enroll
```

Taruh di mesin baru dalam struktur repo (agar `deploy.sh` menemukannya):

```bash
git clone https://github.com/SamVivan1/authFace.git
cd authFace
mkdir -p target/x86_64-unknown-linux-musl/release
cp /path/to/lama/{face-auth,face-enroll} target/x86_64-unknown-linux-musl/release/

sudo ./deploy.sh     # build dilewati, langsung instal
```

GUI (`face-auth-gtk`) **tidak** bisa dicopy begitu karena di-link dinamis ke GTK
→ build di mesin baru (lihat Jalur B langkah 5).

### Jalur B — build di laptop tujuan

#### Dependensi build per distro

```bash
# Fedora / Bluefin / Bazzite / Silverblue (core)
sudo dnf install -y git gcc make musl-gcc curl unzip tar

# Fedora (GUI, opsional)
sudo dnf install -y gtk4-devel libadwaita-devel
```

```bash
# Ubuntu / Debian (core)
sudo apt update && sudo apt install -y git build-essential musl-tools curl unzip

# Ubuntu / Debian (GUI, opsional)
sudo apt install -y libgtk-4-dev libadwaita-1-dev
```

```bash
# Arch / Manjaro (core)
sudo pacman -S --needed git base-devel musl curl unzip

# Arch (GUI, opsional)
sudo pacman -S --needed gtk4 libadwaita
```

> `musl-gcc` / `musl-tools` / `musl` dibutuhkan untuk target build statis.
> Kalau tidak ada, build akan gagal di tahap linking.

#### Rust toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup target add x86_64-unknown-linux-musl
```

Lanjut ke [Instalasi langkah demi langkah](#4-instalasi-langkah-demi-langkah).

### Jalur C — distrobox (distro immutable)

```bash
distrobox create --image docker.io/library/fedora:40 --name authface-dev
distrobox enter authface-dev

# di dalam container
sudo dnf install -y rust cargo gcc gcc-c++ musl-gcc cmake curl unzip \
                    gtk4-devel libadwaita-devel
cd ~/authFace
cargo build --release --target x86_64-unknown-linux-musl -p face-auth -p face-enroll
cargo build --release -p face-auth-gtk

exit
sudo ./deploy.sh       # di HOST
./deploy-gui.sh        # di HOST
```

---

## 4. Instalasi langkah demi langkah

### Langkah 1 — Dapatkan source

```bash
git clone https://github.com/SamVivan1/authFace.git
cd authFace
```

### Langkah 2 — Build binary core

```bash
cargo build --release --target x86_64-unknown-linux-musl -p face-auth -p face-enroll
```

Hasil:
- `target/x86_64-unknown-linux-musl/release/face-auth` (binary PAM)
- `target/x86_64-unknown-linux-musl/release/face-enroll` (CLI enroll)

### Langkah 3 — Install ke sistem

```bash
sudo ./deploy.sh
```

Apa yang dilakukan script ini:

| Tahap | Detail |
|---|---|
| Build | Compile kalau `cargo` ada; kalau tidak, pakai binary yang sudah ada di `target/` |
| Binary | Install ke `/usr/local/bin` (`face-auth`, `face-enroll`) |
| Model deteksi | Download `version-slim-320.onnx` jika belum ada (≈1.2 MB) |
| Model pengenalan | Download `w600k_mbf.onnx` dari InsightFace + verifikasi SHA-256 (≈13 MB) |
| Config | Install `/etc/face-auth.toml` **hanya jika belum ada** (config lama tidak ditimpa) |
| PAM | Tambah baris `auth sufficient pam_exec.so quiet /usr/local/bin/face-auth` ke `sudo`, `gdm-password`, `swaylock` |
| SELinux | Compile + load policy `face_auth` (khusus Fedora/derivatif) |
| Storage | Bikin `/var/lib/face-auth/<user>/` (sticky bit, boleh ditulis user) |

> Setiap file PAM di-backup dulu dengan suffix `.face-auth.bak` → mudah dikembalikan.

**Butuh internet?** Hanya kalau `models/w600k_mbf.onnx` dan
`models/version-slim-320.onnx` belum ada di folder `models/`. Untuk install offline,
siapkan folder itu dulu (lihat [bagian 8](#8-bawa-ke-laptop-lain--install-offline)).

### Langkah 4 — Tes TANPA PAM dulu (penting!)

Jangan langsung andalkan PAM. Tes binary langsung:

```bash
sudo env PAM_USER=$USER USER=$USER HOME=$HOME /usr/local/bin/face-auth
echo "exit=$?"
```

- `exit=1` + pesan `No face detected` / `Face verification failed` → **normal**, kamera
  dan pipeline jalan, tinggal enroll.
- `exit=1` + `Config error` / `No IR camera found` → bantu di [Troubleshooting](#9-troubleshooting).
- `exit=0` → sudah cocok (kemungkinan pernah enroll).

Lihat log detail:

```bash
sudo env PAM_USER=$USER USER=$USER HOME=$HOME RUST_LOG=face_auth_core=debug \
     /usr/local/bin/face-auth
```

### Langkah 5 — Build & install GUI (opsional)

```bash
cargo build --release -p face-auth-gtk
./deploy-gui.sh
```

`deploy-gui.sh` otomatis memilih lokasi:
- `/usr` bisa ditulis (distro biasa) → `/usr/local/bin` + `/usr/share/applications`
- `/usr` read-only (immutable) → `~/.local/bin` + `~/.local/share/applications`

Buka dari menu aplikasi: **Face Authentication Settings**, atau jalankan `face-auth-gtk`.

> Kalau GUI tidak muncul di menu, jalankan `face-auth-gtk` dari terminal untuk lihat error.

### Langkah 6 — (Opsional) Indikator status di lock screen

```bash
extensions/authface-scan-indicator/install-extension.sh
# lalu: log out / log in (Wayland) atau Alt+F2 → r (X11)
```

Butuh GNOME Shell 45+. Muncul di **lock screen session** (Super+L), bukan di layar GDM.

---

## 5. Enroll wajah & verifikasi

### Cara cepat (CLI)

```bash
face-enroll --user $USER -f 8 -v
```

| Flag | Fungsi | Default |
|---|---|---|
| `-u, --user` | user target (**wajib**) | — |
| `-f, --frames` | jumlah frame diambil | 5 |
| `--interval` | jeda antar frame (ms) | 400 |
| `--improve` | **tambah** ke embeddings lama (bukan replace) | off |
| `--device` | pin kamera tertentu | auto |
| `--embeddings-dir` | lokasi penyimpanan | `/var/lib/face-auth` |
| `-v` | verbose | off |

Saat enroll, posisikan wajah seperti waktu dipakai unlock (jarak & sudut yang sama),
diam beberapa detik. Frame gelap/strobe otomatis di-skip.

### Cara enak (GUI)

**Face Authentication Settings** → duduk di depan kamera → lihat preview
(indikator hijau "✓ Face detected") → **Enroll Face** → **Test**.

| Tombol | Fungsi |
|---|---|
| **Enroll Face** | Ganti seluruh embeddings (pakai ini kalau enroll ulang) |
| **Improve Matching** | Tambah embeddings (kondisi cahaya/sudut berbeda) |
| **Test** | Scan 5 detik, bandingkan dengan embeddings tersimpan |

### Kalau sering gagal / serah salah

```bash
# longgarkan threshold (0.6 → 0.55) di ~/.config/face-auth.toml
echo 'threshold = 0.55' >> ~/.config/face-auth.toml

# atau tambah variasi wajah
face-enroll --user $USER --improve -f 8
```

Rekomendasi: **0.6** (default) untuk akurasi; **0.55** kalau banyak false-negative;
**0.65** kalau ada false-positive.

### Cara kerja pipeline (biar paham kenapa hasilnya segitu)

```
kamera IR (GREY 640×360)
  → buang frame strobe (gelap), ambil frame terang
  → histogram equalization
  → deteksi wajah (version-slim-320.onnx, input [-1,1])
  → crop kotak wajah (bujursangkar + margin 35%)
  → resize 112×112, normalisasi [-1,1]
  → MobileFaceNet → embedding 512-d
  → cosine similarity vs embeddings tersimpan (threshold 0.6)
  → exit 0 (match) / exit 1 (fallback password)
```

> **Format embeddings v2**: sejak perubahan face-crop, file `embeddings.bin`
> memakai versi 2. File v1 (hasil build lama) akan ditolak dengan pesan
> *"Embeddings were made by an older version — re-enroll"* → cukup enroll ulang.

---

## 6. Komponen opsional

| Komponen | Perlu? | Install |
|---|---|---|
| Core (`face-auth`) | **Wajib** | `sudo ./deploy.sh` |
| CLI enroll | **Wajib** (ikut deploy.sh) | ikut deploy.sh |
| GUI settings | Sangat disarankan | `./deploy-gui.sh` |
| Indikator lock screen | Opsional | `extensions/authface-scan-indicator/install-extension.sh` |

Multi-user: tiap user enroll sendiri (`face-enroll --user <user>`), masing-masing
punya config & embeddings sendiri (`~/.config/face-auth.toml` dan
`/var/lib/face-auth/<user>/`).

---

## 7. Install ulang / distro hop (backup & restore)

### Yang PERLU dibawa (kecil, penting banget)

```bash
~/.config/face-auth.toml              # config (threshold, device, dll.)
/var/lib/face-auth/<user>/embeddings.bin   # wajah yang sudah di-enroll
```

Backup sebelum install ulang:

```bash
sudo cp -a /var/lib/face-auth ./face-auth-embeddings.bak
cp -a ~/.config/face-auth.toml ./face-auth.toml.bak 2>/dev/null || true
tar czf face-auth-backup.tgz face-auth-embeddings.bak face-auth.toml.bak 2>/dev/null
```

### Yang TIDAK perlu dibawa

- Binary (`target/`) → build ulang atau salin (lihat Jalur A)
- Model ONNX → di-download `deploy.sh`
- `/etc/face-auth.toml` → digenerate ulang oleh deploy
- File PAM → dibuat ulang oleh deploy (dan PAM distro baru juga baru)

### Restore setelah install ulang / pindah distro

```bash
cd authFace

# 1. toolchain + build (atau pakai Jalur A)
cargo build --release --target x86_64-unknown-linux-musl -p face-auth -p face-enroll
cargo build --release -p face-auth-gtk

# 2. install
sudo ./deploy.sh
./deploy-gui.sh

# 3. pulihkan config & embeddings
cp face-auth.toml.bak ~/.config/face-auth.toml
sudo mkdir -p /var/lib/face-auth/$USER
sudo cp -a face-auth-embeddings.bak/<user>/embeddings.bin /var/lib/face-auth/$USER/
sudo chown -R $USER:$USER /var/lib/face-auth/$USER

# 4. tes
sudo env PAM_USER=$USER USER=$USER HOME=$HOME /usr/local/bin/face-auth; echo $?
```

> **Catatan penting**
> - Salin `embeddings.bin` hanya dari build **v2** (pesan error kalau versi beda).
> - Nomor `/dev/videoN` bisa berbeda di laptop/distro lain → kalau salah kamera,
>   set ulang `device` (paling gampang lewat GUI camera picker), atau hapus baris
>   `device` biar auto-detect.
> - Kalau ragu, enroll ulang (5 menit) lebih aman daripada bawa file lama.

### Instalasi bersih dari nol (checklist)

```bash
git clone https://github.com/SamVivan1/authFace.git && cd authFace
rustup target add x86_64-unknown-linux-musl                       # sekali saja
cargo build --release --target x86_64-unknown-linux-musl -p face-auth -p face-enroll
cargo build --release -p face-auth-gtk                            # opsional
sudo ./deploy.sh
./deploy-gui.sh                                                   # opsional
face-enroll --user $USER -f 8 -v
sudo true                                                         # harus exit 0
```

---

## 8. Bawa ke laptop lain / install offline

### Siapkan bundle (di mesin lama, sebelum berangkat)

```bash
cd authFace
cargo build --release --target x86_64-unknown-linux-musl -p face-auth -p face-enroll
cargo build --release -p face-auth-gtk

mkdir -p bundle/target/x86_64-unknown-linux-musl/release bundle/models bundle/target/release
cp target/x86_64-unknown-linux-musl/release/{face-auth,face-enroll} \
   bundle/target/x86_64-unknown-linux-musl/release/
cp target/release/face-auth-gtk bundle/target/release/   # hanya jalan di distro dgn GTK sama
cp models/*.onnx bundle/models/                          # biar tidak download
tar czf authface-bundle.tar.gz bundle deploy.sh deploy-gui.sh uninstall.sh config/ data/ selinux/ extensions/
```

`face-auth`/`face-enroll` statis → aman dibawa ke distro apa pun.
`face-auth-gtk` dinamis → kalau distro tujuan beda, build ulang (butuh GTK 4.14+).

### Install di mesin tujuan (tanpa internet)

```bash
tar xzf authface-bundle.tar.gz && cd authFace
mkdir -p models target/x86_64-unknown-linux-musl/release
cp bundle/models/*.onnx models/                # deploy.sh akan pakai file lokal
cp bundle/target/x86_64-unknown-linux-musl/release/{face-auth,face-enroll} \
   target/x86_64-unknown-linux-musl/release/
sudo ./deploy.sh                               # tidak perlu download
./deploy-gui.sh
face-enroll --user $USER -f 8 -v
```

### Butuh Rust di mesin tujuan?

Tidak, asalkan pakai Jalur A (binary sudah dibawa). Rust hanya dibutuhkan untuk
build dari source atau build GUI.

---

## 9. Troubleshooting

### Gejala → Penyebab → Solusi

| Gejala | Penyebab | Solusi |
|---|---|---|
| `Embeddings were made by an older version — re-enroll` | `embeddings.bin` format v1 dari build lama | `face-enroll --user $USER -f 8` |
| Selalu jatuh ke password, tidak pernah wajah | deteksi/kamera bermasalah | jalankan face-auth manual (langkah 4), lihat log |
| `No IR camera found` | tidak ada kamera IR / bukan grup video | cek `ls /sys/class/video4linux/*/name`, `sudo usermod -aG video $USER` |
| `Camera busy` | aplikasi lain memegang kamera (mis. preview GUI) | tutup GUI/preview lalu coba lagi |
| Preview GUI hitam semua | device menunjuk node RGB/metadata, atau kamera dipegang proses lain | pilih device IR via dropdown GUI; jalankan `examples/diag` |
| Kadang bisa, kadang tidak (build lama) | buffer tunggal + strobe IR | **sudah diperbaiki** — update ke versi ini |
| `sudo` lambat ~1–2 detik | wajar (scan window) | turunkan `scan_duration_ms` di config |
| Model gagal download | tidak ada internet / proxy | taruh `models/*.onnx` manual lalu jalankan ulang `deploy.sh` |
| GUI tidak start | GTK < 4.14 / libadwaita < 1.5 | `pkg-config --modversion gtk4 libadwaita-1`; install versi cukup |
| Lock screen tidak minta wajah (Fedora) | SELinux menolak | `journalctl -k \| grep face-auth \| grep denied`; install `policycoreutils` lalu `sudo ./deploy.sh` lagi |
| Enroll: `No face detected in any frame` | tidak ada wajah / kamera salah | duduk depan kamera, cek device, pakai `-v` |

### Perintah diagnostik

```bash
# 1. Nama semua node kamera
for d in /sys/class/video4linux/*; do echo "$(basename $d): $(cat $d/name)"; done

# 2. Jalankan auth langsung dengan log lengkap
sudo env PAM_USER=$USER USER=$USER HOME=$HOME RUST_LOG=face_auth_core=debug \
     /usr/local/bin/face-auth

# 3. Log internal (buffer kamera, box deteksi, timing)
FACEDIAG=1 sudo -E env PAM_USER=$USER USER=$USER HOME=$HOME /usr/local/bin/face-auth

# 4. Analisis 20 frame (skor deteksi, similarity, box wajah)
cargo run --release -p face-auth-core --example diag -- --frames 20

# 5. Log PAM
journalctl | grep -i "pam_exec\|face-auth"

# 6. Cek apakah PAM sudah terpasang
grep face-auth /etc/pam.d/sudo /etc/pam.d/gdm-password /etc/pam.d/swaylock

# 7. Cek embeddings tersimpan
ls -la /var/lib/face-auth/$USER/
```

### Darurat: kembalikan PAM seperti semula

```bash
sudo cp /etc/pam.d/sudo.face-auth.bak /etc/pam.d/sudo
sudo cp /etc/pam.d/gdm-password.face-auth.bak /etc/pam.d/gdm-password
sudo cp /etc/pam.d/swaylock.face-auth.bak /etc/pam.d/swaylock
# atau sekalian: sudo ./uninstall.sh
```

Karena memakai `sufficient`, **password selalu jalan** sebagai fallback —
gagal wajah tidak pernah mengunci kamu keluar.

---

## 10. Uninstall

```bash
sudo ./uninstall.sh            # hapus core + GUI + model + config + SELinux policy
sudo ./uninstall.sh --gui      # hapus hanya GUI
sudo ./uninstall.sh --purge    # termasuk embeddings wajah
```

Mengembalikan file PAM dari backup `.face-auth.bak`.

---

## 11. Checklist cepat

**Laptop baru / install ulang:**

- [ ] Ada node kamera `IR` (`ls /sys/class/video4linux/*/name`)
- [ ] User sudah di grup `video` (relogin)
- [ ] Clone repo
- [ ] (Jalur B/C) Rust + `rustup target add x86_64-unknown-linux-musl`
- [ ] `cargo build --release --target x86_64-unknown-linux-musl -p face-auth -p face-enroll`
- [ ] `sudo ./deploy.sh`
- [ ] Tes: `sudo env PAM_USER=$USER USER=$USER HOME=$HOME /usr/local/bin/face-auth` → exit 1 (belum enroll) itu normal
- [ ] `face-enroll --user $USER -f 8 -v`
- [ ] Tes: `sudo true` → exit 0
- [ ] (Opsional) `cargo build --release -p face-auth-gtk && ./deploy-gui.sh`
- [ ] (Opsional) `extensions/authface-scan-indicator/install-extension.sh`

**Distro hop / pindah mesin:**

- [ ] Backup `~/.config/face-auth.toml` + `/var/lib/face-auth/<user>/embeddings.bin`
- [ ] Install ulang dari atas
- [ ] Restore config & embeddings (atau enroll ulang)
- [ ] Cek `device` (nomor `/dev/videoN` bisa berubah)
- [ ] Tes `sudo true`
