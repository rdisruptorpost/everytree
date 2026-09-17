//! Developer-only offscreen UI review. No desktop window or Everything connection.
//! cargo test --release --bin everytree render_ui_previews -- --ignored
use super::*;
use eframe::{egui_wgpu, wgpu};

fn fixture() -> App {
    let data =
        Arc::new(model::synthetic(30_000, false, &AtomicBool::new(false), |_, _, _| {}).unwrap());
    App {
        view: Some(View::folder(&data, ROOT)),
        tree: Tree::new(&data),
        data: Some(data),
        job: None,
        query: String::new(),
        source: Source::Everything(String::new()),
        history: History::default(),
        selected: None,
        cache: None,
        depth: 32,
        detail: Detail::Fine,
        error: None,
        notice: None,
        gpu: "Offscreen preview".into(),
        help: false,
        tree_scroll: None,
        shell_result: None,
    }
}

/// Fictional creative-workstation data for the README; no filesystem access.
fn readme_fixture() -> App {
    let mut builder = model::Builder::new(160_000);
    let groups: &[(&str, u64, usize, &[&str])] = &[
        (
            r"D:\Projects\Aurora\Footage\Camera A",
            520,
            360,
            &["crm", "mov", "mp4"],
        ),
        (
            r"D:\Projects\Aurora\Footage\Camera B",
            340,
            240,
            &["mov", "crm", "wav"],
        ),
        (
            r"D:\Projects\Aurora\Renders\Final",
            280,
            12_000,
            &["exr", "png", "exr"],
        ),
        (
            r"D:\Projects\Aurora\Renders\Previews",
            90,
            96,
            &["mp4", "mov"],
        ),
        (
            r"D:\Projects\Aurora\Scenes",
            75,
            220,
            &["blend", "abc", "fbx"],
        ),
        (
            r"D:\Projects\Tidal\Footage",
            410,
            420,
            &["mov", "mp4", "crm"],
        ),
        (
            r"D:\Projects\Tidal\Simulations",
            260,
            3_200,
            &["vdb", "abc", "bin"],
        ),
        (
            r"D:\Projects\Tidal\Textures",
            125,
            8_600,
            &["exr", "tif", "png", "jpg"],
        ),
        (
            r"D:\Projects\Orbit\Renders",
            320,
            14_400,
            &["exr", "png", "mov"],
        ),
        (
            r"D:\Projects\Orbit\Scenes",
            95,
            380,
            &["blend", "fbx", "usd"],
        ),
        (
            r"D:\Asset Library\Megascans\Surfaces",
            290,
            9_800,
            &["tif", "exr", "jpg"],
        ),
        (
            r"D:\Asset Library\Megascans\3D Assets",
            180,
            2_800,
            &["fbx", "obj", "png"],
        ),
        (
            r"D:\Asset Library\Kitbash\Architecture",
            145,
            640,
            &["blend", "fbx", "zip"],
        ),
        (r"D:\Asset Library\HDRI", 60, 460, &["hdr", "exr"]),
        (r"D:\Backups\Projects\2026", 470, 72, &["zip", "7z", "tar"]),
        (r"D:\Backups\Workstation", 280, 180, &["7z", "zip", "bak"]),
        (r"D:\Media\Documentary", 225, 320, &["mkv", "mp4", "mov"]),
        (
            r"D:\Media\Photography\RAW",
            155,
            7_200,
            &["cr3", "dng", "jpg"],
        ),
        (r"D:\Media\Audio", 58, 4_200, &["wav", "flac", "mp3"]),
        (
            r"C:\Users\Demo\Documents\Projects",
            82,
            18_000,
            &["rs", "ts", "json", "dll"],
        ),
        (
            r"C:\Users\Demo\AppData\Local\Packages",
            72,
            15_000,
            &["dll", "bin", "db"],
        ),
        (
            r"C:\Users\Demo\AppData\Local\Cache",
            95,
            21_000,
            &["bin", "dat", "db"],
        ),
        (
            r"C:\Users\Demo\Downloads",
            64,
            880,
            &["zip", "exe", "pdf", "mp4"],
        ),
        (r"C:\Users\Demo\Virtual Machines", 135, 5, &["vhdx", "vdi"]),
        (
            r"C:\Program Files\Creative Tools",
            90,
            14_000,
            &["dll", "exe", "pak"],
        ),
        (r"C:\Windows\WinSxS", 38, 12_000, &["dll", "exe", "dat"]),
    ];
    let mut seed = 0x1234_5678_9abc_def0u64;
    for &(path, gib, count, extensions) in groups {
        let folder = builder.folder(path).unwrap();
        let stem = path.rsplit('\\').next().unwrap().replace(' ', "_");
        let average = gib * 1024 * 1024 * 1024 / count as u64;
        for index in 0..count {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let bytes = average * (seed % 1_000 + 1) / 500;
            let extension = extensions[index % extensions.len()];
            builder
                .add_file(
                    folder,
                    &format!("{stem}_{index:05}.{extension}"),
                    Some(bytes),
                )
                .unwrap();
        }
    }
    let selected = builder.folder(r"D:\Projects\Aurora").unwrap();
    let mut data = builder
        .finish(&AtomicBool::new(false), |_, _, _| {})
        .unwrap();
    data.synthetic = true;
    data.source = "Synthetic / creative workstation".into();
    let data = Arc::new(data);
    let mut app = fixture();
    app.tree = Tree::new(&data);
    app.view = Some(View::folder(&data, ROOT));
    app.select_entry(&data, selected, false);
    app.tree.reveal(&data, selected);
    app.tree.toggle(&data, selected);
    app.data = Some(data);
    app
}

fn loading(app: &mut App, stage: &str, done: usize, total: usize) {
    let (_, rx) = mpsc::channel();
    app.job = Some(Job {
        rx,
        cancel: Arc::new(AtomicBool::new(false)),
        stage: stage.into(),
        done,
        total,
        since: Instant::now() - Duration::from_millis(4300),
        thread: None,
    });
}

#[test]
fn loading_does_not_resize_the_footer_or_shift_the_canvas() {
    for width in [940.0, 1400.0, 1920.0] {
        let ctx = egui::Context::default();
        chrome::set_style(&ctx);
        let mut app = fixture();
        let mut baseline = None;
        for (stage, done, total, cancel) in [
            ("Starting", 0, 0, false),
            ("Building the file hierarchy", 9_000_000, 10_000_000, false),
            ("Sorting folders by size", 900, 1_000, true),
            ("", 0, 0, false),
        ] {
            if stage.is_empty() {
                app.job = None;
            } else {
                loading(&mut app, stage, done, total);
                app.job
                    .as_ref()
                    .unwrap()
                    .cancel
                    .store(cancel, Ordering::Relaxed);
            }
            let mut available = Rect::NOTHING;
            // Panels settle after the first frame; compare settled geometry.
            for _ in 0..2 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(width, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        app.toolbar(ctx);
                        app.status(ctx);
                        available = ctx.available_rect();
                    },
                );
            }
            if let Some(expected) = baseline {
                assert_eq!(available, expected, "{width}px, {stage}");
            } else {
                baseline = Some(available);
            }
        }
    }
}

#[test]
#[ignore = "Writes GPU-rendered UI review images to artifacts/ui; needs a graphics adapter"]
fn render_ui_previews() {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        std::fs::create_dir_all("artifacts/ui").unwrap();
        for (name, width, height, scale, state) in [
            ("readme", 1600, 1050, 1.0, 8),
            ("ready", 1400, 900, 1.0, 0),
            ("compact", 940, 600, 1.0, 0),
            ("loading", 1400, 900, 1.0, 1),
            ("loading-compact", 940, 600, 1.0, 1),
            ("waiting", 940, 600, 1.0, 2),
            ("cancelling", 940, 600, 1.0, 3),
            ("error", 940, 600, 1.0, 4),
            ("hidpi", 1880, 1200, 2.0, 0),
            ("deep-path", 940, 600, 1.0, 5),
            ("tooltip-folder", 1400, 900, 1.0, 6),
            ("tooltip-long-path", 1880, 1200, 2.0, 7),
        ] {
            let ctx = egui::Context::default();
            chrome::set_style(&ctx);
            let mut app = if state == 8 {
                readme_fixture()
            } else {
                fixture()
            };
            let mut tooltip_target = None;
            let data = app.data.clone().unwrap();
            let selected = data.children[data.node(ROOT).child_start as usize];
            if state != 8 {
                app.select_entry(&data, selected, true);
            }
            match state {
                1 => loading(&mut app, "Building the file hierarchy", 8_500_000, 10_105_524),
                2 => { loading(&mut app, "Counting indexed files", 0, 0); app.data = None; },
                3 => { loading(&mut app, "Sorting folders by size", 9_000_000, 10_105_524); app.job.as_ref().unwrap().cancel.store(true, Ordering::Relaxed); },
                4 => app.error = Some("Everything did not respond within 45 seconds. Check that Everything is responding, then retry with a narrower query such as C:\\Users\\. Your existing view is still available.".into()),
                5 => {
                    let id = (1..data.nodes.len() as Id).find(|id| !data.node(*id).is_dir()).unwrap();
                    app.navigate(View::folder(&data, data.node(id).parent));
                    app.select_entry(&data, id, true);
                    app.query = "C:\\Users\\example\\Documents\\Projects\\ ext:mp4".into();
                }
                6 | 7 => {
                    let mut builder = model::Builder::new(2);
                    let path = if state == 6 { "C:\\Users\\c\\Documents\\Claude" } else { "\\\\server\\share\\Projects\\An unusually long folder name with spaces\\Review exports\\A_very_long_folder_name_without_spaces_012345678901234567890123456789\\Claude" };
                    let folder = builder.folder(path).unwrap();
                    builder.add_file(folder, "A large video.mp4", Some(163 * 1024 * 1024 * 1024)).unwrap();
                    builder.add_file(folder, "Project.blend", Some(700 * 1024 * 1024)).unwrap();
                    let data = Arc::new(builder.finish(&AtomicBool::new(false), |_, _, _| {}).unwrap());
                    app.tree = Tree::new(&data);
                    app.view = Some(View::folder(&data, data.node(folder).parent));
                    app.select_entry(&data, folder, true);
                    app.data = Some(data);
                    tooltip_target = Some(folder);
                    ctx.style_mut(|s| s.interaction.tooltip_delay = 0.0);
                }
                _ => {}
            }
            let format = wgpu::TextureFormat::Rgba8Unorm;
            let mut renderer =
                egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());
            let screen = egui_wgpu::ScreenDescriptor {
                size_in_pixels: [width, height],
                pixels_per_point: scale,
            };
            let mut output = egui::FullOutput::default();
            for frame in 0..if tooltip_target.is_some() { 8 } else { 3 } {
                let mut input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(width as f32 / scale, height as f32 / scale),
                    )),
                    time: Some(4.0 + frame as f64 * 0.016),
                    ..Default::default()
                };
                if let Some(target) = tooltip_target
                    && let Some(tile) = app.cache.as_ref().and_then(|cache| {
                        cache
                            .layout
                            .tiles
                            .iter()
                            .find(|t| matches!(t.kind, TileKind::Entry(id) if id == target))
                    })
                {
                    let x = if state == 7 {
                        tile.rect.right() - 10.0
                    } else {
                        tile.rect.left() + 10.0
                    };
                    input
                        .events
                        .push(egui::Event::PointerMoved(pos2(x, tile.rect.top() + 8.0)));
                }
                input
                    .viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .native_pixels_per_point = Some(scale);
                output = ctx.run(input, |ctx| {
                    app.toolbar(ctx);
                    app.status(ctx);
                    if let Some(data) = app.data.clone() {
                        app.filesystem(ctx, &data);
                        egui::CentralPanel::default()
                            .frame(
                                egui::Frame::new()
                                    .fill(BG)
                                    .inner_margin(egui::Margin::symmetric(20, 12)),
                            )
                            .show(ctx, |ui| app.canvas(ui, &data, app.view.unwrap()));
                    } else {
                        egui::CentralPanel::default()
                            .frame(egui::Frame::new().fill(BG))
                            .show(ctx, |_| {});
                    }
                });
                for (id, delta) in &output.textures_delta.set {
                    renderer.update_texture(&device, &queue, *id, delta);
                }
            }
            assert_eq!(output.pixels_per_point, scale);
            let primitives = ctx.tessellate(output.shapes, scale);
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("UI preview"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let mut encoder = device.create_command_encoder(&Default::default());
            let commands =
                renderer.update_buffers(&device, &queue, &mut encoder, &primitives, &screen);
            {
                let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                renderer.render(&mut pass.forget_lifetime(), &primitives, &screen);
            }
            let stride = (width * 4).div_ceil(256) * 256;
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: (stride * height) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(stride),
                        rows_per_image: None,
                    },
                },
                texture.size(),
            );
            queue.submit(commands.into_iter().chain([encoder.finish()]));
            let (tx, rx) = mpsc::channel();
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    tx.send(result).unwrap();
                });
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            rx.recv().unwrap().unwrap();
            let mapped = buffer.slice(..).get_mapped_range();
            let pixels: Vec<u8> = mapped
                .chunks(stride as usize)
                .flat_map(|row| &row[..width as usize * 4])
                .copied()
                .collect();
            image::save_buffer(
                format!("artifacts/ui/{name}.png"),
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    });
}
