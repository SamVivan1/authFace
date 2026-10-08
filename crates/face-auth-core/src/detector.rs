use image::{DynamicImage, ImageBuffer, Luma};
use tract_onnx::prelude::*;
use tract_ndarray::s;

use crate::capture::IrFrame;

pub struct FaceDetector {
    model: InferenceSimplePlan<InferenceModel>,
    threshold: f32,
}

impl FaceDetector {
    pub fn new(model_path: &str, threshold: f32) -> anyhow::Result<Self> {
        let model = onnx()
            .model_for_path(model_path)?
            .into_runnable()?;
        Ok(Self { model, threshold })
    }

    pub fn detect(&mut self, frame: &IrFrame) -> anyhow::Result<bool> {
        Ok(self.detect_face(frame)?.is_some())
    }

    /// Highest-scoring face box above `threshold`, in original-frame pixels.
    ///
    /// `version-slim-320` outputs `[1, N, 2]` class scores and (normally) a
    /// second `[1, N, 4]` tensor of boxes normalised to the 320x240 input.
    pub fn detect_face(&mut self, frame: &IrFrame) -> anyhow::Result<Option<(f32, FaceRect)>> {
        let outputs = self.run_model(frame, -1.0, 1.0)?;
        if outputs.len() < 2 {
            anyhow::bail!("detector produced {} outputs, expected scores + boxes", outputs.len());
        }
        let scores = outputs[0].to_array_view::<f32>()?.to_owned();
        let boxes = outputs[1].to_array_view::<f32>()?.to_owned();

        let face_scores = scores.slice(s![0, .., 1]);
        let mut best_i = 0usize;
        let mut best_s = f32::NEG_INFINITY;
        for (i, v) in face_scores.iter().enumerate() {
            if *v > best_s {
                best_s = *v;
                best_i = i;
            }
        }
        if best_s < self.threshold {
            return Ok(None);
        }

        let b = boxes.slice(s![0, best_i, ..]);
        if b.len() < 4 {
            anyhow::bail!("detector box tensor has {} values, expected 4", b.len());
        }
        let w = frame.width as f32;
        let h = frame.height as f32;
        let rect = FaceRect {
            x1: b[0usize] * w,
            y1: b[1usize] * h,
            x2: b[2usize] * w,
            y2: b[3usize] * h,
        };
        if std::env::var_os("FACEDIAG").is_some() {
            eprintln!(
                "FACEDIAG box raw=[{:.3},{:.3},{:.3},{:.3}] px=[{:.0},{:.0},{:.0},{:.0}] score={:.3}",
                b[0usize], b[1usize], b[2usize], b[3usize], rect.x1, rect.y1, rect.x2, rect.y2, best_s
            );
        }
        Ok(Some((best_s, rect)))
    }

    pub fn score(&mut self, frame: &IrFrame) -> anyhow::Result<f32> {
        // version-slim-320 has no normalisation inside the graph (the first
        // node is a Conv straight on `input`), so we must feed it what the
        // upstream Ultra-Light training pipeline used: (x/255 - 0.5) / 0.5,
        // i.e. [-1, 1]. Feeding [0, 1] collapses the score to ~0.105 on
        // histogram-equalised IR frames, below any sane threshold — which is
        // why detection used to fail on every frame.
        self.score_with_range(frame, -1.0, 1.0)
    }

    pub fn score_with_range(&mut self, frame: &IrFrame, lo: f32, hi: f32) -> anyhow::Result<f32> {
        let outputs = self.run_model(frame, lo, hi)?;
        let scores = outputs[0].to_array_view::<f32>()?;
        let face_scores = scores.slice(s![0, .., 1]);
        Ok(face_scores.iter().cloned().fold(0.0f32, f32::max))
    }

    fn run_model(
        &mut self,
        frame: &IrFrame,
        lo: f32,
        hi: f32,
    ) -> anyhow::Result<Vec<TValue>> {
        let input = preprocess_for_detector(frame, lo, hi)?;
        let mut input = input.into_dyn();
        input.insert_axis_inplace(tract_ndarray::Axis(0));
        let input_tensor = Tensor::from(input).into_tvalue();
        Ok(self.model.run(tvec!(input_tensor))?.into_vec())
    }
}

/// Face bounding box in original-frame pixel coordinates.
#[derive(Debug, Clone, Copy)]
pub struct FaceRect {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

pub fn raw_frame_has_content(frame: &IrFrame) -> bool {
    if frame.data.is_empty() {
        return false;
    }
    let len = frame.data.len() as f32;
    let sum: f64 = frame.data.iter().map(|&v| v as f64).sum();
    let mean = (sum / len as f64) as f32;
    let variance: f32 = frame.data.iter().map(|&v| { let d = (v as f32) - mean; d * d }).sum::<f32>() / len;
    variance > 100_000.0
}

fn preprocess_for_detector(frame: &IrFrame, lo: f32, hi: f32) -> anyhow::Result<tract_ndarray::Array3<f32>> {
    let width = frame.width as u32;
    let height = frame.height as u32;

    let img_buffer: ImageBuffer<Luma<u16>, Vec<u16>> = ImageBuffer::from_fn(width, height, |x, y| {
        let idx = (y * width + x) as usize;
        let val = frame.data.get(idx).copied().unwrap_or(0);
        Luma([val])
    });

    let dynamic_img = DynamicImage::ImageLuma16(img_buffer);
    let resized = dynamic_img.resize_exact(320, 240, image::imageops::FilterType::Lanczos3);
    let rgb = resized.to_rgb8();

    let mut array = tract_ndarray::Array3::<f32>::zeros((3, 240, 320));

    for y in 0..240usize {
        for x in 0..320usize {
            let pixel = rgb.get_pixel(x as u32, y as u32);
            let scale = (hi - lo) / 255.0;
            let r = pixel.0[0] as f32 * scale + lo;
            let g = pixel.0[1] as f32 * scale + lo;
            let b = pixel.0[2] as f32 * scale + lo;
            array[[0, y, x]] = r;
            array[[1, y, x]] = g;
            array[[2, y, x]] = b;
        }
    }

    Ok(array)
}


