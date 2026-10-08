use face_auth_core::capture::Camera;
use face_auth_core::detector::{raw_frame_has_content, FaceDetector};
use face_auth_core::inference::FaceEncoder;
use face_auth_core::preprocess::{histogram_equalize, preprocess_ir_frame};
use face_auth_core::storage::EmbeddingStore;
use face_auth_core::FaceAuthConfig;
use std::time::{Duration, Instant};

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

fn report(
    label: &str,
    frame: &face_auth_core::capture::IrFrame,
    detector: &mut FaceDetector,
    encoder: &mut FaceEncoder,
    store: &EmbeddingStore,
    config: &FaceAuthConfig,
) -> anyhow::Result<()> {
    let len = frame.data.len();
    let mean = frame.data.iter().map(|&v| v as f64).sum::<f64>() / len as f64;
    let var = frame
        .data
        .iter()
        .map(|&v| {
            let d = v as f64 - mean;
            d * d
        })
        .sum::<f64>()
        / len as f64;
    let content = raw_frame_has_content(frame);

    let mut eq = frame.clone();
    histogram_equalize(&mut eq);

    let s_raw_01 = detector.score_with_range(frame, 0.0, 1.0)?;
    let s_raw_neg = detector.score_with_range(frame, -1.0, 1.0)?;
    let s_eq_01 = detector.score_with_range(&eq, 0.0, 1.0)?;
    let s_eq_neg = detector.score_with_range(&eq, -1.0, 1.0)?;

    let face = detector.detect_face(&eq)?;
    let input = preprocess_ir_frame(&eq, face.as_ref().map(|(_, r)| r))?;
    let embedding = encoder.encode(input.view())?;
    let best = store
        .embeddings
        .iter()
        .map(|s| cosine(&embedding, s))
        .fold(f32::MIN, f32::max);

    println!(
        "{:<22} content={:<3} mean={:7.1} std={:8.1} | det raw[0,1]={:.3} raw[-1,1]={:.3} eq[0,1]={:.3} eq[-1,1]={:.3} | sim={:.3} {}",
        label,
        if content { "yes" } else { "no" },
        mean,
        var.sqrt(),
        s_raw_01,
        s_raw_neg,
        s_eq_01,
        s_eq_neg,
        best,
        if store.embeddings.is_empty() {
            ""
        } else if best >= config.threshold() {
            "MATCH"
        } else {
            "no-match"
        }
    );
    Ok(())
}

fn to_frame(data: Vec<u16>, width: u32, height: u32) -> face_auth_core::capture::IrFrame {
    face_auth_core::capture::IrFrame {
        data,
        width,
        height,
        sequence: 0,
    }
}

fn main() -> anyhow::Result<()> {
    let mut device = "/dev/video2".to_string();
    let mut frames: usize = 40;
    let mut interval: u64 = 100;
    let mut user = String::from("samvivan");
    let mut image_path: Option<String> = None;
    let mut raw_path: Option<String> = None;
    let mut raw_wh: (u32, u32) = (640, 360);
    let mut matrix = false;
    let mut reopen = false;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--device" => {
                device = args[i + 1].clone();
                i += 2;
            }
            "--frames" => {
                frames = args[i + 1].parse()?;
                i += 2;
            }
            "--interval" => {
                interval = args[i + 1].parse()?;
                i += 2;
            }
            "--user" => {
                user = args[i + 1].clone();
                i += 2;
            }
            "--image" => {
                image_path = Some(args[i + 1].clone());
                i += 2;
            }
            "--raw" => {
                raw_path = Some(args[i + 1].clone());
                raw_wh = (args[i + 2].parse()?, args[i + 3].parse()?);
                i += 4;
            }
            "--matrix" => {
                matrix = true;
                i += 1;
            }
            "--reopen" => {
                reopen = true;
                i += 1;
            }
            other => anyhow::bail!("unknown arg {}", other),
        }
    }

    let config = FaceAuthConfig::load()?;
    let store = EmbeddingStore::load(&user, &config.embeddings_dir())?;
    let mut encoder = FaceEncoder::new(&config.model_path())?;
    let mut detector = FaceDetector::new(&config.detector_model_path(), config.detector_threshold())?;

    println!(
        "embeddings={} detector_thr={:.2} match_thr={:.2}",
        store.embeddings.len(),
        config.detector_threshold(),
        config.threshold()
    );

    if matrix {
        println!("pairwise cosine similarity of stored embeddings:");
        for (i, a) in store.embeddings.iter().enumerate() {
            for (j, b) in store.embeddings.iter().enumerate() {
                if j > i {
                    println!("  [{}] vs [{}]: {:.3}", i, j, cosine(a, b));
                }
            }
        }
        return Ok(());
    }

    if let Some(path) = image_path {
        let img = image::open(&path)?;
        let gray = img.to_luma16();
        let (w, h) = gray.dimensions();
        let data = gray.into_raw();
        report(
            &format!("image {}", path),
            &to_frame(data, w, h),
            &mut detector,
            &mut encoder,
            &store,
            &config,
        )?;
        return Ok(());
    }

    if let Some(path) = raw_path {
        let bytes = std::fs::read(&path)?;
        let (w, h) = raw_wh;
        let stride = (w * h) as usize;
        let count = bytes.len() / stride;
        for n in 0..count.min(12) {
            let slice = &bytes[n * stride..(n + 1) * stride];
            let data: Vec<u16> = slice.iter().map(|&b| (b as u16) * 257).collect();
            report(
                &format!("raw frame {}", n + 1),
                &to_frame(data, w, h),
                &mut detector,
                &mut encoder,
                &store,
                &config,
            )?;
        }
        return Ok(());
    }

    println!(
        "{:>3} {:>6} {:>7} {:>7} {:>6} {:>6} {:>7} {:>7} {:>7} {:>7}",
        "n", "seq", "mean", "std", "len", "content", "sc01", "sc-11", "sim", "verdict"
    );

    if reopen {
        println!("reopen mode: fresh Camera::open per frame (same as GUI preview)");
        for n in 0..frames {
            let t = Instant::now();
            let frame = match face_auth_core::capture::capture_ir_frame(&device, 5000) {
                Ok(f) => f,
                Err(e) => {
                    println!("{:>3} ERR {}", n + 1, e);
                    continue;
                }
            };
            let len = frame.data.len();
            let mean = frame.data.iter().map(|&v| v as f64).sum::<f64>() / len as f64;
            let var = frame
                .data
                .iter()
                .map(|&v| {
                    let d = v as f64 - mean;
                    d * d
                })
                .sum::<f64>()
                / len as f64;
            let content = raw_frame_has_content(&frame);
            let mut eq = frame;
            histogram_equalize(&mut eq);
            let sc01 = detector.score_with_range(&eq, 0.0, 1.0)?;
            let scneg = detector.score_with_range(&eq, -1.0, 1.0)?;
            println!(
                "{:>3} {:>6} {:>7.1} {:>7.2} {:>6} {:>6} {:>7.3} {:>7.3}  ({:?})",
                n + 1,
                eq.sequence,
                mean,
                var.sqrt(),
                len,
                if content { "yes" } else { "no" },
                sc01,
                scneg,
                t.elapsed()
            );
            std::thread::sleep(Duration::from_millis(interval));
        }
        return Ok(());
    }

    let mut cam = Camera::open(&device)?;
    for n in 0..frames {
        let t = Instant::now();
        let frame = match cam.capture_frame(5000) {
            Ok(f) => f,
            Err(e) => {
                println!("{:>3} ERR {}", n + 1, e);
                continue;
            }
        };

        let len = frame.data.len();
        let mean = frame.data.iter().map(|&v| v as f64).sum::<f64>() / len as f64;
        let var = frame
            .data
            .iter()
            .map(|&v| {
                let d = v as f64 - mean;
                d * d
            })
            .sum::<f64>()
            / len as f64;
        let std = var.sqrt();
        let content = raw_frame_has_content(&frame);
        let seq = frame.sequence;

        if !content {
            println!(
                "{:>3} {:>6} {:>7.1} {:>7.2} {:>6} {:>6} {:>7} {:>7} {:>7} {:>7}  ({:?})",
                n + 1,
                seq,
                mean,
                std,
                len,
                "no",
                "-",
                "-",
                "-",
                "skip",
                t.elapsed()
            );
            std::thread::sleep(Duration::from_millis(interval));
            continue;
        }

        let mut eq = frame;
        histogram_equalize(&mut eq);
        let sc01 = detector.score_with_range(&eq, 0.0, 1.0)?;
        let scneg = detector.score_with_range(&eq, -1.0, 1.0)?;
        let face = detector.detect_face(&eq)?;
        let input = preprocess_ir_frame(&eq, face.as_ref().map(|(_, r)| r))?;
        let embedding = encoder.encode(input.view())?;
        let best = store
            .embeddings
            .iter()
            .map(|s| cosine(&embedding, s))
            .fold(f32::MIN, f32::max);

        let verdict = if best >= config.threshold() {
            "MATCH"
        } else {
            "no-match"
        };
        println!(
            "{:>3} {:>6} {:>7.1} {:>7.2} {:>6} {:>6} {:>7.3} {:>7.3} {:>7.3} {:>7}  ({:?})",
            n + 1,
            seq,
            mean,
            std,
            len,
            "yes",
            sc01,
            scneg,
            best,
            verdict,
            t.elapsed()
        );

        std::thread::sleep(Duration::from_millis(interval));
    }

    Ok(())
}
