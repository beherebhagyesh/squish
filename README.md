# Squish

A Windows desktop app for compressing and converting images. No Python, no terminal, no setup — download the installer and it works.

## Download

**[Download Squish for Windows →](https://github.com/beherebhagyesh/squish/releases/latest)**

Run `Squish_x64-setup.exe`, install it, open from the Start Menu.

---

## What it does

- Pick a source folder or zip file
- Choose how images should be processed
- Convert — results show file sizes and reduction percentage
- Open the output folder directly from the app

## Modes

| Mode | Behavior |
|---|---|
| Keep Format & Compress | JPG stays JPG, PNG stays PNG — just smaller |
| Convert to WebP | Any format → WebP for web publishing |
| Convert & Compress | Change format and reduce size aggressively |

## Presets

| Preset | Quality | Max Width |
|---|---|---|
| Lossless | 100, lossless | No limit |
| Balanced | 82 | 2000px |
| Small File | 68 | 1600px |

## Advanced options

- Quality (1–100)
- Max width / max height
- Target file size in KB (iterative loop — tries to hit the target, reports closest result)
- Strip metadata (EXIF, etc.)
- Output format selector (for Convert & Compress mode)

---

## Supported formats

**Input:** JPG, PNG, WebP, BMP, TIFF, GIF

**Output:** JPG, PNG, WebP

---

## System requirements

- Windows 10 or Windows 11 (x64)
- No other dependencies

---

## Build from source

Requires [Rust](https://rustup.rs) and the Tauri CLI.

```bash
git clone https://github.com/beherebhagyesh/squish
cd squish
cargo install tauri-cli --version "^2"
cargo tauri build
```

Installers are output to:

```
src-tauri/target/release/bundle/nsis/Squish_x64-setup.exe
src-tauri/target/release/bundle/msi/Squish_x64_en-US.msi
```

For development with hot reload:

```bash
cargo tauri dev
```

---

## Project structure

```
squish/
├── ui/                  # Frontend (HTML, CSS, JS)
│   ├── index.html
│   ├── app.js
│   └── styles.css
└── src-tauri/           # Rust backend and Tauri config
    ├── src/
    │   ├── main.rs
    │   ├── lib.rs       # Tauri commands (scan, convert, open_folder)
    │   └── converter.rs # Image conversion engine
    ├── Cargo.toml
    └── tauri.conf.json
```

---

## Related

[compression-script](https://github.com/beherebhagyesh/compression-script) — the browser-based prototype this app was built from.
