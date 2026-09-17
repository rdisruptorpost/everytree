# Local benchmark results

Windows x64, optimized release builds, Everything 1.4.1.1026. These numbers are specific to this machine and index.

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
