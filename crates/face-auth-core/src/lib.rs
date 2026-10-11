pub mod capture;
pub mod config;
pub mod detector;
pub mod error;
pub mod inference;
pub mod preprocess;
pub mod storage;
pub mod user;
pub mod verify;

pub use crate::capture::Camera;
pub use crate::config::FaceAuthConfig;
use crate::detector::{assess_frame, FaceDetector, FrameQuality};
use crate::error::FaceAuthError;
use crate::inference::FaceEncoder;
use crate::storage::EmbeddingStore;
use crate::verify::verify_embedding;
use anyhow::Result;
use std::time::{Duration, Instant};

/// Progress reporting for the interactive enrolment paths.
///
/// The library never writes to stdout itself — `face-auth` runs under
/// `pam_exec`, where stray output lands on the user's terminal on every
/// `sudo`. Callers that *are* interactive supply a sink.
pub type ProgressFn<'a> = &'a mut dyn FnMut(EnrollProgress);

#[derive(Debug, Clone)]
pub enum EnrollProgress {
    Capturing { captured: usize, wanted: usize, attempt: usize },
    NoContent,
    NoFace,
    Captured { captured: usize, wanted: usize },
}

pub struct FaceAuth {
    config: FaceAuthConfig,
    encoder: FaceEncoder,
    detector: FaceDetector,
}

impl FaceAuth {
    pub fn new(config: FaceAuthConfig) -> Result<Self> {
        config.validate()?;
        let encoder = FaceEncoder::new(&config.model_path())?;
        let detector = FaceDetector::new(&config.detector_model_path(), config.detector_threshold())?;
        Ok(Self { config, encoder, detector })
    }

    pub fn config(&self) -> &FaceAuthConfig {
        &self.config
    }

    /// Single-shot verification. Used by the settings GUI's test button; the
    /// PAM path uses [`FaceAuth::authenticate_scan`].
    pub fn authenticate_once(&mut self, user: &str) -> Result<bool> {
        let duration = self.config.scan_duration_ms();
        let interval = self.config.scan_interval_ms();
        self.authenticate_scan(user, duration, interval)
    }

    /// Keep capturing until a frame matches or the scan window closes.
    pub fn authenticate_scan(
        &mut self,
        user: &str,
        duration_ms: u64,
        interval_ms: u64,
    ) -> Result<bool> {
        let t0 = Instant::now();
        let store = EmbeddingStore::load(user, &self.config.embeddings_dir())?;
        eprintln!("TIMING store_load: {:?}", t0.elapsed());

        let t_cam = Instant::now();
        let mut cam = Camera::open(&self.config.device())?;
        eprintln!("TIMING camera_open: {:?}", t_cam.elapsed());

        let deadline = Instant::now() + Duration::from_millis(duration_ms);
        let mut frame_num: usize = 0;
        let mut content_frames: usize = 0;
        let mut consecutive_errors = 0u32;

        loop {
            if Instant::now() >= deadline {
                eprintln!(
                    "SCAN: window elapsed ({} frames, {} with content)",
                    frame_num, content_frames
                );
                return if content_frames > 0 {
                    Ok(false)
                } else {
                    Err(crate::error::FaceAuthError::NoFaceDetected.into())
                };
            }

            frame_num += 1;
            let t_cap = Instant::now();
            let frame = match cam.capture_frame(self.config.capture_timeout_ms()) {
                Ok(f) => {
                    consecutive_errors = 0;
                    f
                }
                Err(e) => {
                    consecutive_errors += 1;
                    eprintln!("SCAN: frame {} capture error — {}", frame_num, e);
                    if consecutive_errors >= 3 {
                        return Err(e);
                    }
                    let sleep = Duration::from_millis(interval_ms)
                        .min(deadline.saturating_duration_since(Instant::now()));
                    std::thread::sleep(sleep);
                    continue;
                }
            };
            eprintln!("TIMING frame_{} capture: {:?}", frame_num, t_cap.elapsed());

            // Dark frame: the sensor strobes the IR emitter, so every other
            // frame is black by design. Its lit twin is next in the queue —
            // grab it immediately instead of waiting out the scan interval.
            if !crate::detector::raw_frame_has_content(&frame) {
                continue;
            }
            content_frames += 1;

            let t2 = Instant::now();
            let mut frame = frame;
            crate::preprocess::histogram_equalize(&mut frame);
            eprintln!("TIMING frame_{} equalize: {:?}", frame_num, t2.elapsed());

            let face = match self.detector.detect_face(&frame)? {
                Some(f) => f,
                None => {
                    let sleep = Duration::from_millis(interval_ms)
                        .min(deadline.saturating_duration_since(Instant::now()));
                    std::thread::sleep(sleep);
                    continue;
                }
            };

            let t3 = Instant::now();
            let input = crate::preprocess::preprocess_ir_frame(&frame, Some(&face.1))?;
            let embedding = self.encoder.encode(input.view())?;
            eprintln!("TIMING frame_{} encode: {:?}", frame_num, t3.elapsed());

            let matched = verify_embedding(&embedding, &store, self.config.threshold())?;
            if matched {
                eprintln!("SCAN: match on frame {} after {:?}", frame_num, t0.elapsed());
                return Ok(true);
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            let sleep = Duration::from_millis(interval_ms).min(remaining);
            if !sleep.is_zero() {
                std::thread::sleep(sleep);
            }
        }
    }

    fn capture_embeddings(
        &mut self,
        cam: &mut Camera,
        store: &mut EmbeddingStore,
        frames: usize,
        interval_ms: u64,
        progress: ProgressFn<'_>,
    ) -> Result<()> {
        let mut captured = 0usize;
        let mut attempts = 0usize;
        let max_attempts = frames * 3;

        while captured < frames && attempts < max_attempts {
            println!("Capturing frame {}/{} (attempt {})...", captured + 1, frames, attempts + 1);
            let frame = cam.capture_frame(self.config.capture_timeout_ms())?;
            attempts += 1;
            progress(EnrollProgress::Capturing { captured, wanted: frames, attempt: attempts });

            if !crate::detector::raw_frame_has_content(&frame) {
                progress(EnrollProgress::NoContent);
                eprintln!("No content in frame, retrying...");
                continue;
            }

            let mut frame = frame;
            crate::preprocess::histogram_equalize(&mut frame);

            let face = match self.detector.detect_face(&frame)? {
                Some(f) => f,
                None => {
                    progress(EnrollProgress::NoFace);
                    eprintln!("No face detected, retrying...");
                    std::thread::sleep(std::time::Duration::from_millis(interval_ms));
                    continue;
                }
            };

            let input = crate::preprocess::preprocess_ir_frame(&frame, Some(&face.1))?;
            let embedding = self.encoder.encode(input.view())?;
            store.add_embedding(embedding);
            captured += 1;
            progress(EnrollProgress::Captured { captured, wanted: frames });

            if captured < frames {
                std::thread::sleep(std::time::Duration::from_millis(interval_ms));
            }
        }

        if store.embeddings.is_empty() {
            return Err(anyhow::anyhow!("No face detected in any frame during enrollment"));
        }

        Ok(())
    }

    /// Replace the user's enrolled embeddings.
    pub fn enroll(
        &mut self,
        user: &str,
        frames: usize,
        interval_ms: u64,
        progress: ProgressFn<'_>,
    ) -> Result<usize> {
        let mut store = EmbeddingStore::default();
        let mut cam = Camera::open(&self.config.device())?;
        self.capture_embeddings(&mut cam, &mut store, frames, interval_ms, progress)?;

        let saved = store.embeddings.len();
        store.save(user, &self.config.embeddings_dir())?;
        Ok(saved)
    }

    /// Append to the user's enrolled embeddings, improving coverage across
    /// lighting and angles.
    pub fn enroll_append(
        &mut self,
        user: &str,
        frames: usize,
        interval_ms: u64,
        progress: ProgressFn<'_>,
    ) -> Result<(usize, usize)> {
        let mut store = match EmbeddingStore::load(user, &self.config.embeddings_dir()) {
            Ok(s) => s,
            // Only "nothing enrolled yet" starts from empty. Any other error
            // (corrupt or unreadable file) must not silently discard what is
            // already there — saving would overwrite it.
            Err(e) if matches!(e.downcast_ref::<FaceAuthError>(), Some(FaceAuthError::NoEmbeddings)) => {
                EmbeddingStore::default()
            }
            Err(e) => return Err(e.context("refusing to append: existing embeddings unreadable")),
        };

        let existing = store.embeddings.len();
        let mut cam = Camera::open(&self.config.device())?;
        self.capture_embeddings(&mut cam, &mut store, frames, interval_ms, progress)?;

        let total = store.embeddings.len();
        store.save(user, &self.config.embeddings_dir())?;
        Ok((total - existing, total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_are_sane() {
        let config = FaceAuthConfig::default();
        assert_eq!(config.threshold(), 0.6);
        assert!(config.validate().is_ok());
    }
}
