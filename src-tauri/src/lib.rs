mod converter;

use converter::{
    ConvertOptions, FileResult, ScannedImage, ScanCounts, ScanResult,
    encode_image, find_images, make_output_path, prepare_source,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Command: scan
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ScanRequest {
    pub source_path: String,
    pub output_base_path: String,
    pub output_folder_name: String,
}

#[tauri::command]
fn scan(req: ScanRequest) -> Result<ScanResult, String> {
    let (working_folder, _label) = prepare_source(&req.source_path)
        .map_err(|e| e.to_string())?;

    let output_base = PathBuf::from(&req.output_base_path);
    if !output_base.exists() || !output_base.is_dir() {
        return Err("Output base path does not exist or is not a directory.".into());
    }
    let folder_name = req.output_folder_name.trim().to_string();
    if folder_name.is_empty() {
        return Err("Output folder name is required.".into());
    }
    let output_root = output_base.join(&folder_name);

    let images = find_images(&working_folder);
    let total = images.len();

    let scanned: Vec<ScannedImage> = images
        .iter()
        .map(|p| {
            let relative = p.strip_prefix(&working_folder)
                .unwrap_or(p.as_path())
                .to_string_lossy()
                .to_string();
            let source_ext = format!(
                ".{}",
                p.extension().unwrap_or_default().to_string_lossy().to_lowercase()
            );
            ScannedImage {
                source: p.to_string_lossy().to_string(),
                relative,
                source_ext,
            }
        })
        .collect();

    Ok(ScanResult {
        working_folder: working_folder.to_string_lossy().to_string(),
        output_root: output_root.to_string_lossy().to_string(),
        counts: ScanCounts { total_images: total },
        images: scanned,
    })
}

// ---------------------------------------------------------------------------
// Command: convert
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ConvertRequest {
    pub images: Vec<ScannedImageInput>,
    pub output_root: String,
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

#[derive(Debug, Deserialize)]
pub struct ScannedImageInput {
    pub source: String,
    pub relative: String,
    pub source_ext: String,
}

#[derive(Debug, Serialize)]
pub struct ConvertResponse {
    pub results: Vec<FileResult>,
    pub count: usize,
}

#[tauri::command]
fn convert(req: ConvertRequest) -> Result<ConvertResponse, String> {
    let output_root = PathBuf::from(&req.output_root);

    let opts = ConvertOptions {
        mode: req.mode.clone(),
        preset: req.preset.clone(),
        output_format: req.output_format.clone(),
        quality: req.quality,
        lossless: req.lossless,
        max_width: req.max_width,
        max_height: req.max_height,
        target_kb: req.target_kb,
        strip_metadata: req.strip_metadata,
    };

    let results: Vec<FileResult> = req.images.iter().map(|item| {
        let source = PathBuf::from(&item.source);
        let source_ext = item.source_ext.trim_start_matches('.').to_lowercase();
        let output_path = make_output_path(
            &output_root,
            &item.relative,
            &source_ext,
            &req.mode,
            req.output_format.as_deref(),
        );

        match encode_image(&source, &output_path, &opts) {
            Ok(r) => r,
            Err(e) => {
                let bytes_in = std::fs::metadata(&source).map(|m| m.len()).unwrap_or(0);
                FileResult {
                    source: item.source.clone(),
                    output: output_path.to_string_lossy().to_string(),
                    source_ext: item.source_ext.clone(),
                    output_ext: output_path.extension()
                        .map(|e| format!(".{}", e.to_string_lossy()))
                        .unwrap_or_default(),
                    bytes_in,
                    bytes_out: 0,
                    reduction_pct: 0.0,
                    warnings: vec![],
                    status: "error".into(),
                    error: Some(e.to_string()),
                }
            }
        }
    }).collect();

    let count = results.len();
    Ok(ConvertResponse { results, count })
}

// ---------------------------------------------------------------------------
// Command: open_folder
// ---------------------------------------------------------------------------

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![scan, convert, open_folder])
        .run(tauri::generate_context!())
        .expect("error while running Squish");
}
