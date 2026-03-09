use anyhow::{Context, Result};
use image::{DynamicImage, ImageFormat};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
pub struct ConvertOptions {
    pub mode: String,
    pub preset: String,
    pub output_format: Option<String>,
    pub quality: Option<u8>,
    pub lossless: Option<bool>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub target_kb: Option<u32>,
    pub strip_metadata: Option<bool>,
}

#[derive(Debug, Serialize, Clone)]
pub struct FileResult {
    pub source: String,
    pub output: String,
    pub source_ext: String,
    pub output_ext: String,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub reduction_pct: f64,
    pub warnings: Vec<String>,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ScanResult {
    pub working_folder: String,
    pub output_root: String,
    pub counts: ScanCounts,
    pub images: Vec<ScannedImage>,
}

#[derive(Debug, Serialize)]
pub struct ScanCounts {
    pub total_images: usize,
}

#[derive(Debug, Serialize, Clone)]
pub struct ScannedImage {
    pub source: String,
    pub relative: String,
    pub source_ext: String,
}

// ---------------------------------------------------------------------------
// Supported extensions
// ---------------------------------------------------------------------------

fn is_image_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "jpg" | "jpeg" | "png" | "webp" | "bmp" | "tif" | "tiff" | "gif"
    )
}

// ---------------------------------------------------------------------------
// Source preparation
// ---------------------------------------------------------------------------

pub fn prepare_source(source_path: &str) -> Result<(PathBuf, String)> {
    let source = PathBuf::from(source_path);
    if !source.exists() {
        anyhow::bail!("Source path does not exist.");
    }
    if source.is_dir() {
        let label = source.file_name().unwrap_or_default().to_string_lossy().to_string();
        return Ok((source, label));
    }
    if source.is_file() {
        let ext = source.extension().unwrap_or_default().to_string_lossy().to_lowercase();
        if ext == "zip" {
            let extract_root = source.parent().unwrap_or(Path::new("."))
                .join(format!("{}__extracted", source.file_stem().unwrap_or_default().to_string_lossy()));
            fs::create_dir_all(&extract_root)?;
            let file = fs::File::open(&source)?;
            let mut archive = zip::ZipArchive::new(file)
                .context("Failed to open zip file")?;
            archive.extract(&extract_root)
                .context("Failed to extract zip")?;
            let label = source.file_stem().unwrap_or_default().to_string_lossy().to_string();
            return Ok((extract_root, label));
        }
    }
    anyhow::bail!("Source must be a folder or a .zip file.");
}

// ---------------------------------------------------------------------------
// Image discovery
// ---------------------------------------------------------------------------

pub fn find_images(root: &Path) -> Vec<PathBuf> {
    let mut images = Vec::new();
    walk_dir(root, &mut images);
    images.sort();
    images
}

fn walk_dir(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_dir(&path, out);
        } else if path.is_file() {
            let ext = path.extension().unwrap_or_default().to_string_lossy().to_string();
            if is_image_ext(&ext) {
                out.push(path);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Output path helpers
// ---------------------------------------------------------------------------

fn mode_suffix(mode: &str) -> &str {
    match mode {
        "keep_format" => "_compressed",
        "to_webp_lossless" => "_lossless",
        "to_webp_lossy" => "_webp",
        "convert_compress" => "_optimized",
        _ => "_optimized",
    }
}

fn output_ext(mode: &str, source_ext: &str, output_format: Option<&str>) -> String {
    match mode {
        "keep_format" => {
            if source_ext == "jpeg" { "jpg".to_string() } else { source_ext.to_string() }
        }
        "to_webp_lossless" | "to_webp_lossy" => "webp".to_string(),
        "convert_compress" => {
            let fmt = output_format.unwrap_or("webp").to_lowercase();
            match fmt.as_str() {
                "jpg" | "jpeg" => "jpg".to_string(),
                "png" => "png".to_string(),
                _ => "webp".to_string(),
            }
        }
        _ => "webp".to_string(),
    }
}

pub fn make_output_path(
    output_root: &Path,
    relative: &str,
    source_ext: &str,
    mode: &str,
    output_format: Option<&str>,
) -> PathBuf {
    let rel = Path::new(relative);
    let ext = output_ext(mode, source_ext, output_format);
    let suffix = mode_suffix(mode);
    let stem = rel.file_stem().unwrap_or_default().to_string_lossy();
    let parent = rel.parent().unwrap_or(Path::new(""));
    output_root.join(parent).join(format!("{}{}.{}", stem, suffix, ext))
}

// ---------------------------------------------------------------------------
// Preset resolution
// ---------------------------------------------------------------------------

fn resolve_preset_defaults(preset: &str) -> (u8, bool, Option<u32>) {
    match preset.to_lowercase().replace(' ', "_").replace('-', "_").as_str() {
        "lossless" => (100, true, None),
        "small_file" => (68, false, Some(1600)),
        _ => (82, false, Some(2000)), // balanced
    }
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

pub fn encode_image(
    source: &Path,
    output: &Path,
    opts: &ConvertOptions,
) -> Result<FileResult> {
    let bytes_in = fs::metadata(source)
        .context("Cannot read source file")?.len();

    let source_ext = source.extension()
        .unwrap_or_default().to_string_lossy().to_lowercase().to_string();
    let out_ext = output.extension()
        .unwrap_or_default().to_string_lossy().to_lowercase().to_string();

    let (preset_quality, preset_lossless, preset_max_w) = resolve_preset_defaults(&opts.preset);
    let quality = opts.quality.unwrap_or(preset_quality);
    let lossless = opts.lossless.unwrap_or(preset_lossless)
        || opts.mode == "to_webp_lossless";
    let max_width = opts.max_width.or(preset_max_w);
    let max_height = opts.max_height;
    let target_kb = opts.target_kb;

    let mut warnings = Vec::new();

    // Warn WebP → WebP in keep_format
    if source_ext == "webp" && out_ext == "webp" && opts.mode == "keep_format" {
        warnings.push("Source is already WebP. Re-encoded with current settings.".into());
    }

    let img = image::open(source).context("Failed to open image")?;

    let bytes_out;
    if let Some(target) = target_kb {
        let (data, warn) = encode_to_target(img, &out_ext, quality, lossless, max_width, max_height, target)?;
        if let Some(w) = warn { warnings.push(w); }
        bytes_out = data.len() as u64;
        fs::create_dir_all(output.parent().unwrap_or(Path::new(".")))?;
        fs::write(output, data)?;
    } else {
        let resized = apply_resize(img, max_width, max_height, 1.0);
        let data = encode_to_bytes(&resized, &out_ext, quality, lossless)?;
        bytes_out = data.len() as u64;
        fs::create_dir_all(output.parent().unwrap_or(Path::new(".")))?;
        fs::write(output, data)?;
    }

    let reduction_pct = if bytes_in > 0 {
        ((1.0 - bytes_out as f64 / bytes_in as f64) * 100.0 * 10.0).round() / 10.0
    } else {
        0.0
    };

    Ok(FileResult {
        source: source.to_string_lossy().to_string(),
        output: output.to_string_lossy().to_string(),
        source_ext: format!(".{}", source_ext),
        output_ext: format!(".{}", out_ext),
        bytes_in,
        bytes_out,
        reduction_pct,
        warnings,
        status: "ok".into(),
        error: None,
    })
}

fn apply_resize(img: DynamicImage, max_w: Option<u32>, max_h: Option<u32>, scale: f64) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    let target_w = (max_w.unwrap_or(w) as f64 * scale) as u32;
    let target_h = (max_h.unwrap_or(h) as f64 * scale) as u32;
    let ratio_w = target_w as f64 / w as f64;
    let ratio_h = target_h as f64 / h as f64;
    let ratio = ratio_w.min(ratio_h).min(1.0);
    if ratio < 1.0 {
        let new_w = (w as f64 * ratio) as u32;
        let new_h = (h as f64 * ratio) as u32;
        img.resize(new_w, new_h, image::imageops::FilterType::Lanczos3)
    } else {
        img
    }
}

fn encode_to_bytes(img: &DynamicImage, ext: &str, quality: u8, lossless: bool) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    match ext {
        "webp" => {
            let encoder = if lossless {
                webp::Encoder::from_image(img)
                    .map_err(|e| anyhow::anyhow!("WebP encoder error: {}", e))?
                    .encode_lossless()
            } else {
                webp::Encoder::from_image(img)
                    .map_err(|e| anyhow::anyhow!("WebP encoder error: {}", e))?
                    .encode(quality as f32)
            };
            return Ok(encoder.to_vec());
        }
        "jpg" | "jpeg" => {
            let rgb = img.to_rgb8();
            let mut jpeg_buf = Vec::new();
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                &mut jpeg_buf, quality,
            );
            encoder.encode_image(&rgb)?;
            return Ok(jpeg_buf);
        }
        "png" => {
            img.write_to(&mut buf, ImageFormat::Png)?;
        }
        _ => {
            img.write_to(&mut buf, ImageFormat::Png)?;
        }
    }
    Ok(buf.into_inner())
}

fn encode_to_target(
    img: DynamicImage,
    ext: &str,
    mut quality: u8,
    lossless: bool,
    max_width: Option<u32>,
    max_height: Option<u32>,
    target_kb: u32,
) -> Result<(Vec<u8>, Option<String>)> {
    let target_bytes = (target_kb as usize) * 1024;
    const MIN_QUALITY: u8 = 35;
    const MAX_ITER: usize = 10;
    let mut scale = 1.0_f64;
    let mut last_data = Vec::new();

    for _ in 0..MAX_ITER {
        let resized = apply_resize(img.clone(), max_width, max_height, scale);
        let data = encode_to_bytes(&resized, ext, quality, lossless)?;
        if data.len() <= target_bytes {
            return Ok((data, None));
        }
        last_data = data;
        if quality > MIN_QUALITY {
            quality = quality.saturating_sub(8).max(MIN_QUALITY);
        } else if scale > 0.40 {
            scale = (scale - 0.12).max(0.40);
        } else {
            break;
        }
    }

    let warn = format!(
        "Could not reach {} KB. Best result: {} KB.",
        target_kb,
        last_data.len() / 1024
    );
    Ok((last_data, Some(warn)))
}
