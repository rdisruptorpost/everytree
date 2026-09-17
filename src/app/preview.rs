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
            let mut app = fixture();
            let mut tooltip_target = None;
            let data = app.data.clone().unwrap();
            let selected = data.children[data.node(ROOT).child_start as usize];
            app.select_entry(&data, selected, true);
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
