# Changelog

## [Unreleased]

### Fixed
- Detector input normalisation: `version-slim-320.onnx` has no normalisation inside
  the graph, so the detector is now fed `[-1, 1]` (was `[0, 1]`, which pinned the
  score at ~0.105 — below `detector_threshold`, so detection never fired)
- Capture queues a ring of 4 mmap buffers before `STREAMON` and requeues each buffer
  immediately, so the IR strobe's alternating dark/lit frames no longer cause a
  parity lock (previously up to 32 consecutive dark frames)
- GUI preview and one-shot auth hold the camera open and stream (GNOME
  Camera/GStreamer style) instead of reopening per frame, which always captured the
  black first frame after `STREAMON` and made the preview render all black
- Scan and enrollment skip dark strobe frames without waiting out the scan interval,
  taking the lit frame already queued
- `deploy.sh` no longer overwrites an existing `/etc/face-auth.toml`, so a pinned
  `device` survives redeploys
- User detection now uses a fallback chain (`PAM_USER` → `USER` → `LOGNAME` → `id -un`), no `setenv`/`env_pass` flags needed
- Removed `timeout=10` from PAM stanzas (causes pam_exec to block on stdin; face-auth reads the camera, not stdin)
- `deploy.sh` no longer wipes `/var/lib/face-auth/` on redeploy (preserves enrolled users)
- Model checksum mismatch now aborts deployment instead of continuing with potentially corrupted model
- User config (`~/.config/face-auth.toml`) now correctly overrides system config (`/etc/face-auth.toml`)
- `face-enroll` validates that the target user exists before attempting enrollment
- Camera buffer mmap changed to `PROT_READ` only (principle of least privilege)
- Pinned `image` crate to `0.25.4` (addresses known soundness issues in 0.25.x)

### Changed
- Embeddings are cropped to the detector's face box (padded square) before encoding
  instead of squeezing the whole 640x360 frame into 112x112: same-session cosine
  similarity rose from 0.649 (min 0.521) to 0.893 (min 0.853) against a 0.6 threshold
- Embedding file format bumped to v2; v1 files are rejected with an actionable
  "re-enroll" message instead of being silently mixed with incompatible vectors
- `authenticate_once` (GUI Test) runs the same retrying scan window as PAM and
  distinguishes `No face detected` from `no match`
- Preview runs face detection on every third frame to keep the preview near the
  sensor's own frame rate (~100 ms per inference)

### Added
- `INSTALL.md` — full installation guide: new machine, distro hop, reinstall,
  offline bundle, backup/restore, troubleshooting
- `FACEDIAG=1` diagnostics (buffer queueing, dequeue sequence, detection boxes)
- `face-auth-core` example `diag` (`--frames`, `--reopen`, `--image`, `--raw`, `--matrix`)
- `IrFrame.sequence` for tracking driver frame drops
- `uninstall.sh --purge` flag to optionally remove user embeddings
- Architecture and security limitation documentation in README
- CHANGELOG.md

### Changed
- Clarified SELinux policy scope and trade-offs in documentation
- Added x86_64-only architecture warning to V4L2 capture module
