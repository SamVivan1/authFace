use image::{DynamicImage, ImageBuffer, Luma};
use tract_onnx::prelude::tract_ndarray::Array3;

use crate::detector::FaceRect;

/// Embedding input for the recognition model.
///
/// When the detector box is given, only that face (expanded to a square with
/// margin) is resized to 112x112. Feeding the whole 640x360 scene squeezes
/// the face into a third of the input, so the embedding mostly encodes the
/// background: same-person frames then scored only ~0.6 against each other,
/// which sits right on the verification threshold.
pub fn preprocess_ir_frame(
    frame: &super::capture::IrFrame,
    face: Option<&FaceRect>,
) -> anyhow::Result<Array3<f32>> {
    let width = frame.width as u32;
    let height = frame.height as u32;

    let img_buffer: ImageBuffer<Luma<u16>, Vec<u16>> = ImageBuffer::from_fn(width, height, |x, y| {
        let idx = (y * width + x) as usize;
        let val = frame.data.get(idx).copied().unwrap_or(0);
        Luma([val])
    });

    let (ox, oy, cw, ch) = match face {
        Some(r) => square_face_crop(r, width, height),
        None => (0, 0, width, height),
    };
    let cropped = image::imageops::crop_imm(&img_buffer, ox, oy, cw, ch).to_image();
    let dynamic_img = DynamicImage::ImageLuma16(cropped);
    let resized = dynamic_img.resize_exact(112, 112, image::imageops::FilterType::Lanczos3);
    let gray_img = resized.to_luma16();
    
    let mut array = Array3::<f32>::zeros((3, 112, 112));
    
    for y in 0..112usize {
        for x in 0..112usize {
            let pixel = gray_img.get_pixel(x as u32, y as u32).0[0] as f32 / 65535.0;
            let normalized = (pixel - 0.5) / 0.5;
            for c in 0..3usize {
                array[[c, y, x]] = normalized;
            }
        }
    }
    
    Ok(array)
}

pub fn histogram_equalize(frame: &mut super::capture::IrFrame) {
    let mut hist = [0u32; 65536];
    
    for &val in &frame.data {
        hist[val as usize] += 1;
    }
    
    let total = frame.data.len() as f32;
    let mut cdf = [0f32; 65536];
    let mut sum = 0f32;
    
    for i in 0..65536 {
        sum += hist[i] as f32;
        cdf[i] = sum / total;
    }
    
    for val in &mut frame.data {
        *val = (cdf[*val as usize] * 65535.0) as u16;
    }
}
/// Largest square centred on the face box, padded by 35% so the chin and
/// hairline survive, clamped to the frame.
fn square_face_crop(r: &FaceRect, width: u32, height: u32) -> (u32, u32, u32, u32) {
    let cx = (r.x1 + r.x2) * 0.5;
    let cy = (r.y1 + r.y2) * 0.5;
    let mut side = (r.x2 - r.x1).max(r.y2 - r.y1) * 1.35;
    side = side.max(32.0).min(width as f32).min(height as f32);

    let x0 = (cx - side * 0.5).round().clamp(0.0, (width as f32 - side).max(0.0));
    let y0 = (cy - side * 0.5).round().clamp(0.0, (height as f32 - side).max(0.0));
    (x0 as u32, y0 as u32, side as u32, side as u32)
}
