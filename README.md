# 🚀 Rust-Editor

> The world's most powerful Friday Night Funkin' chart editor written in Rust. Lightweight, highly optimized, and built for performance.

Welcome to **Rust-Editor!** A modern, native Friday Night Funkin' chart editor recreated entirely in Rust.

Designed to be extremely lightweight and highly optimized, Rust-Editor brings a familiar chart-editing experience with a modern dark interface, powerful editing tools, and support for large key modes.

And yes, it has dark mode. We don't want you seeing heaven again.

**Current version:** `v0.0.2`

## ✨ Features

- **Modern Dark Mode** – A sleek graphite interface with dark panels and readable contrast.
- **Highly Optimized** – Built with Rust for performance and lightweight resource usage.
- **1K–32K Key Modes** – Supports up to 64 lanes across both sides.
- **Blue Center Divider** – Clearly separates opponent and player lanes.
- **Advanced Chart Editing** – Place, select, delete, and extend notes.
- **Section Editor** – Navigate, clear, copy, paste, swap sides, stretch, shift, and duplicate sections.
- **Chart Utilities** – Note-spam generation, EZ Spam, and range-based note operations.
- **Song Controls** – Edit song metadata and load/play audio.
- **Import & Export** – Supports Psych Engine-style chart JSON and `FNFEBIN1`, `FNFEBIN2`, and `FNFEBIN3` binary formats.
- **Included Assets** – Original note, grid, font, and character assets.

## 📱 Android Support

Android support is planned for a future release!

The goal is to bring Rust-Editor's chart editing experience to mobile devices while maintaining its lightweight performance and usability.

**Coming soon... hopefully.**

## 🛠️ Build and Run

Install [Rust](https://www.rust-lang.org/tools/install) with a recent stable toolchain (Rust 1.81 or newer).

Clone or download the repository, open a terminal in the project directory, and run:

```bash
cargo run --release
```

To compile without launching the application:

```bash
cargo build --release
```

### 🐧 Linux Dependencies

On Debian/Ubuntu, install the following dependencies:

```bash
sudo apt-get update
sudo apt-get install --yes \
  build-essential \
  pkg-config \
  libasound2-dev \
  libxkbcommon-dev \
  libwayland-dev \
  libx11-dev \
  libxrandr-dev \
  libxi-dev \
  libxcursor-dev \
  libxinerama-dev \
  libgl1-mesa-dev
```

## ⌨️ Controls

| Key / Action | Function |
|---|---|
| `Space` | Start or pause playback |
| Left Click | Place or select a note |
| Right Click | Delete a note near the clicked grid cell |
| `E` / `P` | Extend a note with a sustain |
| `Z` / `X` | Zoom the chart grid out / in |
| `N` / `M` | Adjust camera zoom |
| `J` / `L` | Move the camera horizontally |
| `I` / `K` | Move the grid camera vertically |
| `W` / `S` | Move playback position by one step |
| `A` / `D` | Previous / next section |
| `R` | Swap the two lane sides for the current section |
| Mouse Wheel | Scroll through the chart |

## 🌍 Cross-Platform Support

Rust-Editor is designed with cross-platform compatibility in mind.

The GitHub Actions workflow in `.github/workflows/main.yml` is configured to build release packages for:

| Platform | Architecture | Package |
|---|---|---|
| Windows | x86-64 | `.zip` |
| Linux | x86-64 | `.tar.gz` |
| macOS | Apple Silicon / ARM64 | `.tar.gz` |
| macOS | Intel / x86-64 | `.tar.gz` |

To trigger the release workflow:

```bash
git tag v0.0.1
git push origin v0.0.1
```

Builds and releases are produced through GitHub Actions. Actual release availability depends on successful builds.

## ⚠️ Notes & Limitations

Rust-Editor aims to recreate the original chart editor's layout and functionality, but it is not currently a verified pixel-perfect recreation of the original Java Swing implementation.

Extremely large charts may consume more memory than the original implementation because notes and chart JSON are currently held in memory rather than using the Java version's memory-mapped streaming approach.

Each supported platform should be independently built and tested before considering its release verified.

## ❤️ Credits

Made with Rust, egui, and a lot of hard work.

Thanks for checking out **Rust-Editor!**

*More features, more optimizations, and more platforms are on the way.*
