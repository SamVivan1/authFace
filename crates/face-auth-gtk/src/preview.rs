use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use face_auth_core::capture::{Camera, IrFrame};
use face_auth_core::detector::{raw_frame_has_content, FaceDetector};
use face_auth_core::preprocess::histogram_equalize;

#[derive(Clone)]
pub struct PreviewFrame {
    pub data: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub face_detected: bool,
}

pub struct CaptureController {
    shutdown: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    sender: mpsc::Sender<PreviewFrame>,
    pub receiver: mpsc::Receiver<PreviewFrame>,
    device: String,
    model_path: String,
    detector_model: String,
    detector_threshold: f32,
    capture_timeout: i32,
}

impl CaptureController {
    pub fn new(
        model_path: &str,
        detector_model: &str,
        detector_threshold: f32,
        device: &str,
        capture_timeout: i32,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            shutdown: Arc::new(AtomicBool::new(false)),
            thread: None,
            sender: tx,
            receiver: rx,
            device: device.to_string(),
            model_path: model_path.to_string(),
            detector_model: detector_model.to_string(),
            detector_threshold,
            capture_timeout,
        }
    }

    pub fn set_device(&mut self, device: &str) {
        self.device = device.to_string();
        self.restart();
    }

    pub fn start(&mut self) {
        self.shutdown.store(false, Ordering::Relaxed);
        let shutdown = self.shutdown.clone();
        let tx = self.sender.clone();
        let device = self.device.clone();
        let model_path = self.model_path.clone();
        let detector_model = self.detector_model.clone();
        let detector_threshold = self.detector_threshold;
        let capture_timeout = self.capture_timeout;

        self.thread = Some(thread::spawn(move || {
            let _ = &model_path;
            let mut detector = match FaceDetector::new(&detector_model, detector_threshold) {
                Ok(d) => d,
                Err(_) => return,
            };

            // The camera is opened once and left streaming, the way GNOME
            // Camera/GStreamer does it. Re-opening per frame (what this used
            // to do) always captured the first frame after STREAMON, and that
            // one is always black: the IR emitter needs a beat to come up and
            // the sensor alternates dark/lit frames — hence a black preview.
            let mut camera: Option<Camera> = None;
            let mut errors = 0u32;
            let mut frame_idx: u32 = 0;
            let mut face_detected = false;

            loop {
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }

                if camera.is_none() {
                    match Camera::open(&device) {
                        Ok(c) => {
                            camera = Some(c);
                            errors = 0;
                        }
                        Err(e) => {
                            eprintln!("preview: {}", e);
                            thread::sleep(Duration::from_millis(100));
                            continue;
                        }
                    }
                }

                let frame = match camera.as_mut().unwrap().capture_frame(capture_timeout) {
                    Ok(f) => {
                        errors = 0;
                        f
                    }
                    Err(e) => {
                        errors += 1;
                        eprintln!("preview: capture error — {}", e);
                        if errors >= 3 {
                            camera = None;
                        }
                        thread::sleep(Duration::from_millis(100));
                        continue;
                    }
                };

                // Dark strobe frame: its lit twin is already queued, take it.
                if !raw_frame_has_content(&frame) {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }

                let mut frame = frame;
                histogram_equalize(&mut frame);

                // Detection only: the preview indicator means "a face is in
                // view", so encoding the frame here bought nothing and doubled
                // the per-frame cost. It is also the expensive part (~100ms of
                // tract inference), so it runs on every third frame to keep the
                // preview near the sensor's own frame rate.
                frame_idx = frame_idx.wrapping_add(1);
                if frame_idx % 3 == 1 {
                    face_detected = detector.detect(&frame).unwrap_or(false);
                }

                send_preview(&tx, &frame, face_detected);
                thread::sleep(Duration::from_millis(50));
            }
        }));
    }

    pub fn stop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }

    fn restart(&mut self) {
        self.stop();
        self.start();
    }
}

impl Drop for CaptureController {
    fn drop(&mut self) {
        self.stop();
    }
}

fn send_preview(tx: &mpsc::Sender<PreviewFrame>, frame: &IrFrame, face_detected: bool) {
    let data = frame_to_rgba(frame);
    let _ = tx.send(PreviewFrame {
        data,
        width: frame.width as i32,
        height: frame.height as i32,
        face_detected,
    });
}

fn frame_to_rgba(frame: &IrFrame) -> Vec<u8> {
    let w = frame.width as usize;
    let h = frame.height as usize;
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let pixel = frame.data.get(idx).copied().unwrap_or(0);
            let v = (pixel >> 8) as u8;
            rgba.push(v);
            rgba.push(v);
            rgba.push(v);
            rgba.push(255);
        }
    }
    rgba
}
