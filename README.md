# everytree

A native Rust/egui disk explorer powered by the running [Everything](https://www.voidtools.com/) desktop client. The treemap is rendered by egui through wgpu, navigation and hovering never scan the full file index.

![everytree showing a folder tree, selected folder details, and a colourful disk-usage treemap](assets/screenshot.png)

## Usage

- Double-click a folder or grouped treemap region to drill in.
- Single-click a rectangle to reveal and select its row in the filesystem tree above the treemap.
- Expand folders with their arrows. Click a tree row to focus the treemap on that folder or the file's siblings.
- The details pane shows full location, logical size, file type, parent folder, and sibling count. Drag the divider below the filesystem pane to resize it.
- **Back / Forward**, **Mouse 4 / Mouse 5**, or **Alt+Left / Alt+Right** move through folder history, restoring the selected entry too. Backspace and Shift+Backspace also work when not editing text. Breadcrumbs jump to ancestors. Opening another folder after going back clears the old forward branch, just like a browser; a successful index reload starts fresh history.
- **F5** reloads the current source.
- **Depth** supports up to 64 nested levels, starting at 32. Small folders keep expanding without spending space on headers. Screen space and the tile budget still limit detail; zoom into a folder to reveal regions too small to draw.
- **Detail** is separate from depth: **Balanced** uses a 16,000-tile cap and approximately 16 square points per tile; **Fine** (default) uses 75,000 / 4; **Pixel** uses 200,000 / 1. These are grouping thresholds, not a guarantee of an exact tile size. Higher detail costs more CPU/GPU work. Pixels smaller than a screen point and entries beyond the cap remain grouped.
- Folder sizes appear beside names in both the treemap headers and filesystem tree. Long names are shortened to leave room for the size; unknown totals retain their `+ ?` marker.
- Colours are stable by extension, with ten type families, variations within each family, and hashed hues for other extensions. Folder headers are neutral; file tiles have a subtle cushion highlight.
- **About > Diagnostics** contains import, memory, renderer, and layout statistics. The main interface shows file/folder counts and navigation controls.
- **Explorer** opens the parent and selects the item. **Open folder** opens the selected folder or a file's containing folder. Both actions are also available in the right-click menu. Paths with spaces, commas, and Unicode use native Windows shell item IDs.
- Search, **Refresh**, and **About** share a single toolbar. Loading uses a fixed-height footer with reserved columns for the stage, progress, counts, elapsed time, and Cancel. Messages can be dismissed. Vector icons scale with display DPI, and selection actions stay visible while details scroll.
- Right-click tree rows or treemap rectangles for Explorer, copy path, and navigation actions.
- The filesystem tree is virtualized: even a million siblings only create visible rows. Its expanded projection uses compact ranges instead of allocating a row per file.

## Build from source

Requires Windows x64, Rust installed through rustup, and the Visual C++ build tools / Windows SDK. `rust-toolchain.toml` pins Rust 1.98.1 (with rustfmt and Clippy) for local and CI builds; rustup installs and selects it automatically when you run Cargo in this repository.

```powershell
.\scripts\setup.ps1
cargo run --release --locked
```

`scripts/setup.ps1` downloads the official Everything SDK from `https://www.voidtools.com/Everything-SDK.zip` into the ignored `vendor/` directory. Its DLL is dynamically loaded from beside the executable or the development SDK directory. No system installation is performed.

```powershell
.\scripts\build.ps1
```

The build script runs release tests, builds explicitly for Windows x64/MSVC, and places `everytree.exe`, `Everything64.dll`, the icon, and documentation in `dist/<version>/`. It also creates `dist/everytree-<version>-windows-x64.zip` and a matching `.zip.sha256` checksum, then updates the original `dist/` launch path. If that copy is running, the versioned package and ZIP remain available. Run `.\scripts\build.ps1 -Check` to include formatting and Clippy checks, matching CI.

The folder/treemap icon is embedded in the executable and supplied to the app window. A standalone `everytree.ico` is included for shortcuts. The Windows SDK resource compiler is detected automatically; set `RC` to its `rc.exe` path for a custom installation. Packaged artwork and its generation prompt live in [assets/README.md](assets/README.md); Python/Pillow are only needed when regenerating the icon assets.

## Memory use

The renderer uses smaller GPU allocation blocks and retains geometry only for the visible treemap. File records use 24 bytes, with child ranges and recursive counts stored only for folders. Full filenames and 64-bit sizes remain available at every detail level.

See [BENCHMARKS.md](BENCHMARKS.md#memory-optimization-2026-09-17) for before/after memory and timing measurements. Developers can run `.\scripts\measure-memory.ps1` after building to measure a fresh synthetic-data instance; it closes its own test instance afterward. Actual memory use varies with file count, names, resolution, DPI, and detail level.

## Dependencies

The Everything SDK is supplied by voidtools; consult its bundled source headers for its license. Application code is MIT licensed. Rust dependency versions are pinned in `Cargo.lock`.
