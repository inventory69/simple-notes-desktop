use crate::error::{AppError, Result};
use exif::{In, Tag, Value};
use image::{DynamicImage, GenericImageView, ImageDecoder};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::path::Path;

/// Längste Kante, auf die Compressed/Lossless herunterskaliert wird (Android-Parität).
const MAX_DIMENSION: u32 = 1920;
const LOSSY_QUALITY: f32 = 80.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionMode {
    Compressed,
    Lossless,
    Original,
}

impl CompressionMode {
    pub fn parse(s: &str) -> Self {
        match s {
            "lossless" => Self::Lossless,
            "original" => Self::Original,
            _ => Self::Compressed,
        }
    }
}

/// Verarbeitet ein eingefügtes Rohbild gemäß Kompressions-Modus.
///
/// `Original`: Bytes 1:1 durchreichen (EXIF/GPS bleibt erhalten).
/// `Compressed`/`Lossless`: EXIF-Orientierung vor dem Re-Encode anwenden, auf max.
/// `MAX_DIMENSION`px lange Kante herunterskalieren (nur wenn größer), dann als WebP
/// encoden — das Re-Encode strippt EXIF/GPS automatisch (Privacy-Nebeneffekt).
pub fn process(bytes: &[u8], src_ext: &str, mode: CompressionMode) -> Result<(Vec<u8>, String)> {
    if mode == CompressionMode::Original {
        return Ok((bytes.to_vec(), normalize_ext(src_ext)));
    }

    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| AppError::Image(e.to_string()))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| AppError::Image(e.to_string()))?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img =
        DynamicImage::from_decoder(decoder).map_err(|e| AppError::Image(e.to_string()))?;
    img.apply_orientation(orientation);

    let (w, h) = img.dimensions();
    if w > MAX_DIMENSION || h > MAX_DIMENSION {
        img = img.resize(
            MAX_DIMENSION,
            MAX_DIMENSION,
            image::imageops::FilterType::Lanczos3,
        );
    }

    let encoder = webp::Encoder::from_image(&img).map_err(|e| AppError::Image(e.to_string()))?;
    let encoded = if mode == CompressionMode::Lossless {
        encoder.encode_lossless()
    } else {
        encoder.encode(LOSSY_QUALITY)
    };

    Ok((encoded.to_vec(), "webp".to_string()))
}

fn normalize_ext(ext: &str) -> String {
    let e = ext.trim_start_matches('.').to_ascii_lowercase();
    if e.is_empty() {
        "bin".to_string()
    } else {
        e
    }
}

/// Content-adressierter Dateiname: erste 8 Bytes des SHA-256-Hash als Hex (16 Zeichen) + Endung.
pub fn content_name(bytes: &[u8], ext: &str) -> String {
    let hash = Sha256::digest(bytes);
    let hex: String = hash.iter().take(8).map(|b| format!("{:02x}", b)).collect();
    format!("{}.{}", hex, ext)
}

/// EXIF-Metadaten eines Bild-Assets. Port von Android `ImageMetadata.kt`. `None`-Felder werden
/// im Info-Dialog übersprungen.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageMetadata {
    pub width_px: u32,
    pub height_px: u32,
    pub file_size_bytes: u64,
    pub date_taken: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub iso: Option<u32>,
    pub exposure_time: Option<String>,
    pub focal_length_mm: Option<f64>,
    pub gps: Option<(f64, f64)>,
    /// Re-encodete WebPs strippen EXIF beim Kompressions-Re-Encode → alle Felder `None`.
    pub has_exif: bool,
}

const FAST_SHUTTER_THRESHOLD_S: f64 = 1.0;

fn format_exposure_time(seconds: f64) -> String {
    if seconds >= FAST_SHUTTER_THRESHOLD_S {
        format!("{:.1} s", seconds)
    } else {
        format!("1/{} s", (1.0 / seconds).round() as i64)
    }
}

fn field_string(exif: &exif::Exif, tag: Tag) -> Option<String> {
    exif.get_field(tag, In::PRIMARY)
        .map(|f| f.display_value().to_string())
}

fn field_rational(exif: &exif::Exif, tag: Tag) -> Option<f64> {
    match &exif.get_field(tag, In::PRIMARY)?.value {
        Value::Rational(v) => v.first().map(|r| r.to_f64()),
        _ => None,
    }
}

fn field_uint(exif: &exif::Exif, tag: Tag) -> Option<u32> {
    match &exif.get_field(tag, In::PRIMARY)?.value {
        Value::Short(v) => v.first().map(|&n| n as u32),
        Value::Long(v) => v.first().copied(),
        _ => None,
    }
}

fn gps_coord(exif: &exif::Exif, tag: Tag, ref_tag: Tag, negative_ref: &str) -> Option<f64> {
    let Value::Rational(dms) = &exif.get_field(tag, In::PRIMARY)?.value else {
        return None;
    };
    if dms.len() < 3 {
        return None;
    }
    let decimal = dms[0].to_f64() + dms[1].to_f64() / 60.0 + dms[2].to_f64() / 3600.0;
    if !decimal.is_finite() {
        return None;
    }
    let sign = if field_string(exif, ref_tag).as_deref() == Some(negative_ref) {
        -1.0
    } else {
        1.0
    };
    Some(decimal * sign)
}

/// Liest Dimensionen (bounds-only, kein Full-Decode) + EXIF-Tags aus `path`. `None` bei
/// fehlender/kaputter Datei (kein Error — der Info-Dialog blendet sich dann einfach aus).
pub fn read_metadata(path: &Path) -> Option<ImageMetadata> {
    eprintln!("[images] read_metadata: {}", path.display());

    let (width_px, height_px) = match image::image_dimensions(path) {
        Ok(dim) => dim,
        Err(e) => {
            eprintln!(
                "[images] image_dimensions failed for {}: {}",
                path.display(),
                e
            );
            return None;
        }
    };
    let file_size_bytes = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(e) => {
            eprintln!("[images] fs::metadata failed for {}: {}", path.display(), e);
            return None;
        }
    };

    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[images] File::open failed for {}: {}", path.display(), e);
            return None;
        }
    };
    let mut bufreader = std::io::BufReader::new(&file);
    // ponytail: strict parser aborts with zero fields on the first non-fatal error (common with
    // vendor MakerNote quirks in real camera JPEGs); continue_on_error recovers whatever parsed.
    let exif = match exif::Reader::new()
        .continue_on_error(true)
        .read_from_container(&mut bufreader)
        .or_else(|e| {
            e.distill_partial_result(|errors| {
                eprintln!(
                    "[images] exif partial result for {}, ignored errors: {:?}",
                    path.display(),
                    errors
                );
            })
        }) {
        Ok(exif) => {
            eprintln!(
                "[images] exif parsed for {}: {} field(s)",
                path.display(),
                exif.fields().count()
            );
            Some(exif)
        }
        Err(e) => {
            eprintln!("[images] exif read failed for {}: {}", path.display(), e);
            None
        }
    };

    let (date_taken, camera_make, camera_model, iso, exposure_time, focal_length_mm, gps) =
        match &exif {
            Some(exif) => (
                field_string(exif, Tag::DateTimeOriginal),
                field_string(exif, Tag::Make),
                field_string(exif, Tag::Model),
                field_uint(exif, Tag::PhotographicSensitivity).filter(|&v| v > 0),
                field_rational(exif, Tag::ExposureTime)
                    .filter(|&v| v > 0.0)
                    .map(format_exposure_time),
                field_rational(exif, Tag::FocalLength).filter(|&v| v > 0.0),
                gps_coord(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef, "S").zip(gps_coord(
                    exif,
                    Tag::GPSLongitude,
                    Tag::GPSLongitudeRef,
                    "W",
                )),
            ),
            None => (None, None, None, None, None, None, None),
        };

    let has_exif = date_taken.is_some()
        || camera_make.is_some()
        || camera_model.is_some()
        || iso.is_some()
        || exposure_time.is_some()
        || focal_length_mm.is_some()
        || gps.is_some();

    eprintln!(
        "[images] {} -> has_exif={} date={:?} make={:?} model={:?} iso={:?} exposure={:?} focal={:?} gps={:?}",
        path.display(),
        has_exif,
        date_taken,
        camera_make,
        camera_model,
        iso,
        exposure_time,
        focal_length_mm,
        gps
    );

    Some(ImageMetadata {
        width_px,
        height_px,
        file_size_bytes,
        date_taken,
        camera_make,
        camera_model,
        iso,
        exposure_time,
        focal_length_mm,
        gps,
        has_exif,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_png(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        });
        let mut buf = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    #[test]
    fn test_compression_mode_parse() {
        assert_eq!(
            CompressionMode::parse("compressed"),
            CompressionMode::Compressed
        );
        assert_eq!(
            CompressionMode::parse("lossless"),
            CompressionMode::Lossless
        );
        assert_eq!(
            CompressionMode::parse("original"),
            CompressionMode::Original
        );
        assert_eq!(
            CompressionMode::parse("garbage"),
            CompressionMode::Compressed
        );
    }

    #[test]
    fn test_process_original_passes_through_unchanged() {
        let bytes = make_png(4, 4);
        let (out, ext) = process(&bytes, "png", CompressionMode::Original).unwrap();
        assert_eq!(out, bytes);
        assert_eq!(ext, "png");
    }

    #[test]
    fn test_process_compressed_produces_webp() {
        let bytes = make_png(8, 8);
        let (out, ext) = process(&bytes, "png", CompressionMode::Compressed).unwrap();
        assert_eq!(ext, "webp");
        assert_eq!(&out[0..4], b"RIFF");
        assert_eq!(&out[8..12], b"WEBP");
    }

    #[test]
    fn test_process_lossless_produces_webp() {
        let bytes = make_png(8, 8);
        let (out, ext) = process(&bytes, "png", CompressionMode::Lossless).unwrap();
        assert_eq!(ext, "webp");
        assert_eq!(&out[0..4], b"RIFF");
    }

    #[test]
    fn test_process_invalid_bytes_errors() {
        let result = process(b"not an image", "png", CompressionMode::Compressed);
        assert!(result.is_err());
    }

    #[test]
    fn test_content_name_deterministic_and_format() {
        let bytes = b"hello world";
        let name1 = content_name(bytes, "webp");
        let name2 = content_name(bytes, "webp");
        assert_eq!(name1, name2);
        assert!(name1.ends_with(".webp"));
        let hex_part = name1.strip_suffix(".webp").unwrap();
        assert_eq!(hex_part.len(), 16);
        assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_content_name_differs_for_different_bytes() {
        let a = content_name(b"aaaa", "webp");
        let b = content_name(b"bbbb", "webp");
        assert_ne!(a, b);
    }

    #[test]
    fn test_read_metadata_synthetic_png_has_no_exif() {
        let dir = std::env::temp_dir().join(format!("snd-images-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plain.png");
        std::fs::write(&path, make_png(12, 8)).unwrap();

        let meta = read_metadata(&path).expect("metadata should be readable");
        assert_eq!(meta.width_px, 12);
        assert_eq!(meta.height_px, 8);
        assert!(!meta.has_exif);
        assert!(meta.date_taken.is_none());
        assert!(meta.gps.is_none());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn test_read_metadata_missing_file_returns_none() {
        let path = std::path::Path::new("/nonexistent/does-not-exist.png");
        assert!(read_metadata(path).is_none());
    }
}
