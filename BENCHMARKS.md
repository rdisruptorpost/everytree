# Local benchmark results

Windows x64, optimized release builds, Everything 1.4.1.1026. These numbers are specific to this machine and index.

## Memory optimization (2026-09-17)

Compared the unmodified 0.3.8 checkout against the memory changes using the same pinned Rust 1.98.1 release configuration on this Windows system. These are local measurements, not cross-machine guarantees.

Changes:

- Select wgpu's `MemoryHints::MemoryUsage` while preserving egui's device limits, adapter selection, cached rendering, and tile budgets. In the pinned wgpu-hal 27.0.4 DX12 backend this changes device/host allocation blocks from 256/64 MiB to 8/4 MiB. These are allocation granularity, not memory caps; larger resources still work.
- File records shrink from 32 to 24 bytes. Only folders have a separate 12-byte child-range/file-count record. Savings are approximately 8 bytes per file minus 4 bytes per folder (including the root). Names, 64-bit sizes, prefix sums, and constant-time range totals remain intact. The worker's private transfer protocol is updated to carry the compact representation.

### Idle application

Identical `--demo 1000000` views, default Fine detail, default window size and system DPI. Baseline and optimized instances were launched sequentially, allowed to settle for 8 seconds, then sampled three times. Values below are medians; GPU counters sum the test PID's adapter instances.

| Counter | Before | After |
|---|---:|---:|
| Dedicated GPU memory | 357.8 MiB | 117.7 MiB |
| Shared GPU memory | 102.9 MiB | 49.1 MiB |
| Process working set | 260.0 MiB | 203.2 MiB |
| Process private bytes (committed memory) | 571.3 MiB | 267.2 MiB |

Dedicated GPU memory fell by about 67%. Windows GPU and process counters measure different things and can overlap; do not add them together. Window resolution, DPI, drivers, selected adapter, navigation history, and detail level affect the result. Working set is resident process memory; private bytes are not another measure of physical RAM in use.

### CPU/data comparison

Three runs per build and dataset, alternating build order without overlapping benchmark processes. Each run builds the layout 100 times per detail at 1600 by 900 and depth 32. Table entries are medians across the three runs (including the per-run p50/p95 statistics). Peaks also include the headless layout, mesh, text, and tessellation work.

| Measurement | Before | After |
|---|---:|---:|
| 10 million, mixed: data buffers | 582.7 MiB | 506.5 MiB |
| 10 million, mixed: peak headless working set | 639.8 MiB | 563.1 MiB |
| 10 million, mixed: generate/build time | 1.221 s | 1.216 s |
| 10 million, mixed: Balanced layout p50 / p95 | 1.13 / 1.30 ms | 1.21 / 2.22 ms |
| 10 million, mixed: Fine layout p50 / p95 | 48.11 / 56.85 ms | 48.26 / 52.41 ms |
| 10 million, mixed: Pixel layout p50 / p95 | 132.21 / 155.47 ms | 131.65 / 140.96 ms |
| 1 million, flat: data buffers | 58.1 MiB | 50.4 MiB |
| 1 million, flat: peak headless working set | 119.4 MiB | 111.7 MiB |
| 1 million, flat: generate/build time | 0.168 s | 0.152 s |
| 1 million, flat: Balanced layout p50 / p95 | 12.16 / 13.11 ms | 12.48 / 13.74 ms |
| 1 million, flat: Fine layout p50 / p95 | 31.78 / 36.76 ms | 31.78 / 33.95 ms |
| 1 million, flat: Pixel layout p50 / p95 | 59.20 / 67.87 ms | 56.85 / 59.39 ms |

Fine and Pixel CPU layout times remain close to baseline. The small Balanced case has a higher p95 by less than 1 ms; these measurements do not establish GPU frame throughput or performance on other adapters. All compared runs preserve total bytes, visible tile counts, individual-file/group counts, visited ranges, and geometry size. The GPU-rendered README fixture is pixel-identical to the existing screenshot.

Validation: formatting and Clippy (`-D warnings`) pass, along with 23 tests and the opt-in GPU preview test. A read-only Everything import of the repository captured 12,782 files and 2,123 folders, including 24 unknown-size files, and completed the new worker transfer successfully.

Reproduce after building; pipe the GUI executable's output so PowerShell waits for headless benchmarks:

```powershell
.\scripts\measure-memory.ps1 -Executable .\dist\everytree.exe -Files 1000000
.\dist\everytree.exe --bench 10000000 | Out-Host
.\dist\everytree.exe --bench 1000000 --flat | Out-Host
```

The measurement helper starts a fresh synthetic-data instance, reports byte counters, and closes only that instance. It does not query Everything. GPU fields are empty when the Windows counters are unavailable. Raw measurements are local/ignored in `artifacts/memory/final-*.txt` and `artifacts/memory/*-gpu-final.json`.

## v0.3 granular treemap

Live index: 8,953,142 files / 1,091,789 derived folders, 626.8 MiB data buffers, imported in 9.346 s. All detail modes used the same captured data at 1600 by 900, depth 32.

| Detail | Visible tiles | Individual files | Grouped regions | CPU layout p95 | Cache preparation | CPU tessellation | Geometry |
|---|---:|---:|---:|---:|---:|---:|---:|
| Balanced | 16,000 | 2,144 | 9,094 | 6.64 ms | 2.44 ms | 0.52 ms | 2.1 MiB |
| Fine | 74,999 | 13,957 | 45,179 | 26.36 ms | 7.28 ms | 1.26 ms | 7.7 MiB |
| Pixel | 195,430 | 84,181 | 79,950 | 50.08 ms | 23.03 ms | 3.29 ms | 19.6 MiB |

The remaining tiles are directories. Individual files are actual file entries, not estimated members of groups. Grouping still represents entries below the area threshold or beyond the tile/depth cap. Fine is the default; Pixel trades more preparation and rendering work for finer detail.

Layout p95 covers 100 builds. Cache preparation is one mesh/text build. Layout and mesh/text generation are reused on hover; the CPU tessellation figure excludes GPU execution, uploads, presentation, and the rest of the UI. These are not frame-rate measurements. A synthetic benchmark ran during part of this check, so timings include local CPU contention and are indicative rather than an isolated performance comparison.

The million-sibling synthetic case remained bounded at 75,000 tiles in Fine (layout p95 38.36 ms) and 200,000 in Pixel (67.35 ms). Sixteen tests pass, including small folders deeper than eight levels, tile-budget redistribution, exact byte/area conservation, and dense mesh validity. Aggregate logs: `artifacts/v030-live.txt` and `artifacts/v030-flat.txt`.

## Live import: v0.1 versus v0.2

| Measurement | v0.1 baseline | v0.2 |
|---|---:|---:|
| Indexed files captured | 9,008,446 | 9,008,516 |
| Derived folders | 1,097,267 | 1,097,283 |
| Total load, including worker transfer | 14.648 s | 8.960 s |
| Final data buffers | 630.5 MiB | 630.5 MiB |
| SDK helper peak working set | 1.10 GiB | 1.11 GiB |

The measured pair improved by 38.8%. The live index changed between and during the runs, so these are comparable captures, not an identical frozen dataset or a statistically controlled benchmark.

The v0.2 run spent 6.316 s on 20 SDK queries, 2.124 s decoding/building the hierarchy, and 0.316 s finalizing its indexes; the remaining time includes helper startup and transfer. It recorded six count/position changes and completed successfully. This counter is not a count of changed files.

Changes: 500,000-file bounded pages (previously 250,000), directory decoding only on directory changes, reused filename buffers, and 128 exact page anchors replacing full previous-page hash sets. Native SDK reply time remains the largest measured component.

Raw aggregate output: `artifacts/v02-baseline.txt` and `artifacts/v02-optimized.txt` (local, ignored). No filenames are printed by the benchmark.

## Earlier synthetic measurements

These v0.1 measurements exercise the unchanged model/layout, without SDK queries or process transfer.

| Dataset | Build time | Final data buffers | Peak process working set | Layout p95 |
|---|---:|---:|---:|---:|
| 1 million files, mixed hierarchy | 0.129 s | 58.3 MiB | 66.8 MiB | 0.082 ms |
| 10 million files, mixed hierarchy | 1.309 s | 582.7 MiB | 590.6 MiB | 0.221 ms |
| 1 million files in one folder | 0.185 s | 58.1 MiB | 65.9 MiB | 2.129 ms |

Layout measurements cover 100 CPU layout builds at 1600 by 900 with a 6,000-tile budget. They exclude drawing, GPU execution, event processing, and presentation. They are **not frame-rate measurements**. Normal navigation caches the layout; hovering does not rebuild it.

Memory figures exclude the separate Everything process and GPU memory. Peaks from the UI and helper processes are not necessarily simultaneous. Reproduce with the commands in README.md.
