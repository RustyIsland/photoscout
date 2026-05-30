# PhotoScout

**PhotoScout** is a native Rust desktop app for scanning photo folders, browsing image collections, searching by filename or path, and safely reviewing exact duplicate photos.

It is built with Rust, `eframe/egui`, and a safety-first duplicate cleanup workflow.

> Current status: **v0.1.0 early release**  
> Platform focus: **Windows x64**  
> Cleanup model: **system Trash only — no permanent delete fallback**

---

## Features

- Select and scan one or more photo folders.
- Optional multi-folder library view.
- Browse photos by folder.
- Search by filename or full path.
- Scan recursively for common image formats:
  - JPG / JPEG
  - PNG
  - WEBP
  - BMP
  - GIF
- Ignore tiny files or small image assets using size and dimension thresholds.
- Extract image dimensions and file size.
- Detect exact duplicate files using BLAKE3 content hashing.
- Optimize duplicate detection by hashing only files that share the same file size.
- Review duplicate groups visually.
- Choose an explicit keeper before duplicate cleanup is allowed.
- Move reviewed duplicate copies to the system Trash.
- Open files, open containing folders, and copy file paths.
- Lazy visible-first thumbnail generation.
- Separate worker queues for fast preview thumbnails and refined thumbnails.

---

## Safety Model

PhotoScout is designed to avoid destructive mistakes.

PhotoScout **does not permanently delete files**.

Duplicate Cleanup requires:

1. An exact duplicate match using BLAKE3 content hashing.
2. A duplicate group with two or more matching files.
3. The user manually selecting which file to keep.
4. A confirmation modal before cleanup.
5. Successful movement to the operating system Trash.

If moving a file to Trash fails, PhotoScout shows an error and does **not** fall back to permanent deletion.

---

## Download and Run

### Option 1: Download the Windows release

1. Go to the project’s GitHub **Releases** page.
2. Download the latest file named similar to:

```text
PhotoScout-v0.1.0-windows-x64.zip
```

3. Extract the zip file.
4. Double-click:

```text
photoscout.exe
```

5. Click **+ Select & Scan Folder** to begin.

### Windows SmartScreen note

Windows may show a SmartScreen warning because this is an unsigned new app.
Only run PhotoScout if you downloaded it from the official release page.

---

## Build From Source

### Requirements

- Rust stable toolchain
- Cargo
- Windows, Linux, or macOS desktop environment supported by `eframe/egui`

Install Rust from:

```text
https://www.rust-lang.org/tools/install
```

### Build and run

From the project folder:

```bash
cargo run --release
```

### Build an executable

```bash
cargo build --release
```

The release binary will be created at:

```text
target/release/photoscout.exe
```

On Linux/macOS, the binary will be created at:

```text
target/release/photoscout
```

---

## Diagnostics

Diagnostics and benchmark logs are disabled by default.
Normal runs should not create `benchmark/` folders.

Enable summary diagnostics:

```bash
PHOTOSCOUT_DIAGNOSTICS=summary cargo run --release
```

On PowerShell:

```powershell
$env:PHOTOSCOUT_DIAGNOSTICS="summary"
cargo run --release
```

Enable deep per-thumbnail JSONL diagnostics:

```powershell
$env:PHOTOSCOUT_DIAGNOSTICS="deep"
cargo run --release
```

Deep diagnostics create:

```text
benchmark/run_<timestamp>/events.jsonl
benchmark/run_<timestamp>/summary.txt
```

Diagnostic logs intentionally avoid personal filenames and full folder paths. Image references use session-local photo IDs and generic metadata.

---

## Project Layout

```text
src/
  app/
    mod.rs
    panels.rs
    grid.rs
    duplicate_grid.rs
    cleanup.rs
    helpers.rs
    theme.rs
  thumbnails/
    mod.rs
    bench.rs
    workers.rs
    resize.rs
    types.rs
  diagnostics.rs
  image_decoders.rs
  scanner.rs
  scan_coordinator.rs
  duplicates.rs
  search.rs
  model.rs
  path_utils.rs
  library_roots.rs
  error.rs
  main.rs
```

---

## Current Limitations

PhotoScout v0.1.0 is intentionally focused and conservative.

It currently does **not** include:

- Similar-image detection.
- AI image search.
- Face recognition.
- Image tagging.
- Metadata editing.
- Permanent deletion.
- Automatic duplicate cleanup.
- A persistent database.
- Cloud sync.

Duplicate detection is based on exact file content, not visual similarity.

---

## Recommended First-Time Workflow

1. Test PhotoScout on a small folder first.
2. Scan a copied test folder before using it on important photo libraries.
3. Use Duplicate Review to inspect exact duplicate groups.
4. Select one keeper inside a duplicate group.
5. Confirm cleanup only when the listed files are safe to move to Trash.
6. Check your system Trash before emptying it.

---

## Development Checks

Before creating a release build, run:

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo build --release
```

---

## License

MIT License

---

## Author

Created by **Rusty Island**.

