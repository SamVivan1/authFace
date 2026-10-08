pub mod capture;
pub mod config;
pub mod detector;
pub mod error;
pub mod inference;
pub mod preprocess;
pub mod storage;
pub mod verify;

pub use crate::capture::Camera;
pub use crate::config::FaceAuthConfig;
use crate::detector::FaceDetector;
use crate::inference::FaceEncoder;
use crate::storage::EmbeddingStore;
use crate::verify::verify_embedding;
use anyhow::Result;
use std::time::{Duration, Instant};

pub struct FaceAuth {
    config: FaceAuthConfig,
    encoder: FaceEncoder,
    detector: FaceDetector,
}

impl FaceAuth {
    pub fn new(config: FaceAuthConfig) -> Result<Self> {
        let encoder = FaceEncoder::new(&config.model_path())?;
        let detector = FaceDetector::new(
            &config.detector_model_path(),
            config.detector_threshold(),
        )?;
        Ok(Self { config, encoder, detector })
    }

    /// Single-shot auth used by the GUI "Test" button.
    ///
    /// It is really a short scan: the camera is opened once and kept streaming
    /// for `scan_duration_ms`, because the sensor emits alternating dark/lit
    /// frames (IR strobe) and the first frames after STREAMON are always dark.
    /// Grabbing exactly one frame — as this used to do — reliably grabbed a
    /// dark one, so the test failed no matter where you looked.
    pub fn authenticate_once(&mut self, user: &str) -> Result<bool> {
        let duration = self.config.scan_duration_ms();
        let interval = self.config.scan_interval_ms();
        self.authenticate_scan(user, duration, interval)
    }

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
    ) -> Result<()> {
        let mut captured = 0usize;
        let mut attempts = 0usize;
        let max_attempts = frames * 3;

        while captured < frames && attempts < max_attempts {
            println!("Capturing frame {}/{} (attempt {})...", captured + 1, frames, attempts + 1);
            let frame = cam.capture_frame(self.config.capture_timeout_ms())?;
            attempts += 1;

            if !crate::detector::raw_frame_has_content(&frame) {
                // Strobe frame — the lit twin is next in the queue, take it now.
                eprintln!("No content in frame, retrying...");
                continue;
            }

            let mut frame = frame;
            crate::preprocess::histogram_equalize(&mut frame);

            let face = match self.detector.detect_face(&frame)? {
                Some(f) => f,
                None => {
                    eprintln!("No face detected, retrying...");
                    std::thread::sleep(std::time::Duration::from_millis(interval_ms));
                    continue;
                }
            };

            let input = crate::preprocess::preprocess_ir_frame(&frame, Some(&face.1))?;
            let embedding = self.encoder.encode(input.view())?;
            store.add_embedding(embedding);
            captured += 1;

            if captured < frames {
                std::thread::sleep(std::time::Duration::from_millis(interval_ms));
            }
        }

        if store.embeddings.is_empty() {
            return Err(anyhow::anyhow!("No face detected in any frame during enrollment"));
        }

        Ok(())
    }

    pub fn enroll(&mut self, user: &str, frames: usize, interval_ms: u64) -> Result<()> {
        let mut store = EmbeddingStore::default();
        let mut cam = Camera::open(&self.config.device())?;
        self.capture_embeddings(&mut cam, &mut store, frames, interval_ms)?;

        let saved = store.embeddings.len();
        store.save(user, &self.config.embeddings_dir())?;
        println!("Saved {} embeddings for user '{}'", saved, user);

        Ok(())
    }

    pub fn enroll_append(&mut self, user: &str, frames: usize, interval_ms: u64) -> Result<()> {
        let mut store = match EmbeddingStore::load(user, &self.config.embeddings_dir()) {
            Ok(s) => s,
            Err(_) => EmbeddingStore::default(),
        };
        let existing = store.embeddings.len();
        let mut cam = Camera::open(&self.config.device())?;
        self.capture_embeddings(&mut cam, &mut store, frames, interval_ms)?;

        let total = store.embeddings.len();
        store.save(user, &self.config.embeddings_dir())?;
        println!("Added {} new embeddings for user '{}' ({} total)", total - existing, user, total);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_config_defaults() {
        let config = FaceAuthConfig::default();
        assert_eq!(config.threshold(), 0.6);
        assert!(config.device().starts_with("/dev/video"));
    }
}