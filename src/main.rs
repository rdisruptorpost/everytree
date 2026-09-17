#![cfg_attr(windows, windows_subsystem = "windows")]
mod app;
use everytree::{
    layout::{self, Detail, View},
    model::{self, ROOT, format_bytes, format_count},
    transfer,
};
use std::{sync::atomic::AtomicBool, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--import-worker") {
        transfer::worker(args.get(1).map(String::as_str).unwrap_or(""))?;
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "everytree\n\n  everytree                    Open the live Everything index\n  everytree --query 'C:\\'       Limit the Everything search\n  everytree --demo 1000000      Open synthetic files\n  everytree --demo 1000000 --flat\n  everytree --bench 10000000    Headless synthetic benchmark\n  everytree --bench-live        Headless Everything import\n\nAdd --query to --bench-live to limit the import. Benchmarks print aggregate metrics only."
        );
        return Ok(());
    }
    let value = |flag: &str| -> Result<Option<&str>, String> {
        args.iter()
            .position(|a| a == flag)
            .map(|i| {
                args.get(i + 1)
                    .filter(|a| !a.starts_with("--"))
                    .map(String::as_str)
                    .ok_or_else(|| format!("Missing value after {flag}"))
            })
            .transpose()
    };
    let count = |flag: &str| -> Result<Option<usize>, String> {
        value(flag)?
            .map(|s| {
                s.parse::<usize>()
                    .map_err(|_| format!("Invalid file count: {s}"))
            })
            .transpose()
            .and_then(|n| {
                if n.is_some_and(|n| n > 50_000_000) {
                    Err("Choose at most 50 million synthetic files.".into())
                } else {
                    Ok(n)
                }
            })
    };
    let flat = args.iter().any(|a| a == "--flat");
    let query = value("--query")?.unwrap_or("").to_owned();
    let bench_count = count("--bench")?;
    if bench_count.is_some() || args.iter().any(|a| a == "--bench-live") {
        let cancel = AtomicBool::new(false);
        let start = Instant::now();
        let mut last_stage = String::new();
        let progress = |stage: &str, _done: usize, _total: usize| {
            if stage != last_stage {
                eprintln!("{stage}…");
                last_stage = stage.to_owned();
            }
        };
        let data = if let Some(n) = bench_count {
            model::synthetic(n, flat, &cancel, progress)?
        } else {
            transfer::load(&query, &cancel, progress)?
        };
        println!(
            "Source: {}\nFiles: {}\nFolders: {}\nLogical bytes: {}\nUnknown sizes: {}\nLoad time: {:.3}s\nData buffers: {} ({} bytes)",
            data.source,
            format_count(data.node(ROOT).file_count as usize),
            format_count(data.folder_count()),
            data.node(ROOT).bytes,
            data.unknown_sizes,
            start.elapsed().as_secs_f64(),
            format_bytes(data.storage_bytes() as u64),
            data.storage_bytes()
        );
        let rect = eframe::egui::Rect::from_min_size(
            eframe::egui::Pos2::ZERO,
            eframe::egui::vec2(1600.0, 900.0),
        );
        if data.load_stats.queries > 0 {
            println!(
                "SDK query time: {:.3}s ({} queries)\nDecode/build time: {:.3}s\nFinalize time: {:.3}s\nIndex realignments: {}",
                data.load_stats.query_us as f64 / 1e6,
                data.load_stats.queries,
                data.load_stats.decode_us as f64 / 1e6,
                data.load_stats.finalize_us as f64 / 1e6,
                data.load_stats.changes
            );
        }
        for detail in [Detail::Balanced, Detail::Fine, Detail::Pixel] {
            let mut timings = Vec::new();
            let mut last = layout::Layout::default();
            for _ in 0..100 {
                let start = Instant::now();
                last = layout::build(&data, View::folder(&data, ROOT), rect, detail.settings(32));
                timings.push(start.elapsed().as_secs_f64() * 1000.0);
                std::hint::black_box(&last);
            }
            timings.sort_by(f64::total_cmp);
            let ctx = eframe::egui::Context::default();
            let mut render = everytree::treemap::Render::default();
            let mut prepare_ms = 0.0;
            let output = ctx.run(
                eframe::egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ctx| {
                    let painter = ctx.layer_painter(eframe::egui::LayerId::background());
                    let start = Instant::now();
                    render = everytree::treemap::Render::prepare(&painter, &data, &last);
                    prepare_ms = start.elapsed().as_secs_f64() * 1000.0;
                    render.paint(&painter);
                },
            );
            let start = Instant::now();
            let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
            let tessellate_ms = start.elapsed().as_secs_f64() * 1000.0;
            std::hint::black_box(primitives);
            println!(
                "Detail: {} (depth 32)\nLayout p50: {:.3}ms\nLayout p95: {:.3}ms\nVisible tiles: {}\nIndividual files: {}\nGrouped regions: {}\nVisited ranges: {}\nCached mesh/text preparation: {:.3}ms\nMesh storage: {}\nCPU tessellation: {:.3}ms (excludes GPU rendering)",
                detail.label(),
                timings[50],
                timings[95],
                last.tiles.len(),
                render.files,
                render.groups,
                last.visited,
                prepare_ms,
                format_bytes(render.mesh.bytes_used() as u64),
                tessellate_ms
            );
        }
        #[cfg(windows)]
        if let Some(bytes) = transfer::peak_working_set() {
            println!(
                "Peak process working set: {} ({} bytes)",
                format_bytes(bytes as u64),
                bytes
            );
        }
        if data.import_peak_memory > 0 {
            println!(
                "Peak SDK worker working set (separate process): {} ({} bytes)",
                format_bytes(data.import_peak_memory),
                data.import_peak_memory
            );
        }
        return Ok(());
    }
    let source = count("--demo")?
        .map(|n| app::Source::Sample(n, flat))
        .unwrap_or(app::Source::Everything(query));
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(concat!("everytree ", env!("CARGO_PKG_VERSION")))
            .with_icon(eframe::egui::IconData {
                rgba: include_bytes!("../assets/everytree.rgba").to_vec(),
                width: 256,
                height: 256,
            })
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([940.0, 600.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "everytree",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, source)))),
    )?;
    Ok(())
}
