use eframe::egui::{
    self, Align, Align2, Color32, FontId, Layout as UiLayout, Rect, RichText, Sense, Stroke,
    StrokeKind, pos2, vec2,
};
mod chrome;
mod filesystem;
mod icons;
mod navigation;
#[cfg(test)]
mod preview;
mod tooltip;
use everytree::{
    layout::{self, Detail, TileKind, View},
    model::{self, Dataset, Id, ROOT, format_bytes, format_count},
    transfer,
    tree::Tree,
    treemap::{self, Render, paint_folder_label},
};
use icons::{Icon, IconButton};
use navigation::{Direction, History, Visit};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

const BG: Color32 = Color32::from_rgb(20, 23, 25);
const PANEL: Color32 = Color32::from_rgb(27, 30, 32);
const TEXT: Color32 = Color32::from_rgb(232, 233, 227);
const MUTED: Color32 = Color32::from_rgb(151, 158, 159);
const BORDER: Color32 = Color32::from_rgb(48, 54, 57);
const ACCENT: Color32 = Color32::from_rgb(236, 171, 107);

enum Event {
    Progress(String, usize, usize),
    Done(Result<Box<Dataset>, String>),
}
#[derive(Clone)]
pub enum Source {
    Everything(String),
    Sample(usize, bool),
}
struct Job {
    rx: Receiver<Event>,
    cancel: Arc<AtomicBool>,
    stage: String,
    done: usize,
    total: usize,
    since: Instant,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        // Disconnect before joining: a final message must not block on a full queue.
        let (_, empty) = mpsc::channel();
        drop(std::mem::replace(&mut self.rx, empty));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
struct Cache {
    view: View,
    rect: Rect,
    depth: u8,
    detail: Detail,
    pixels_per_point: f32,
    layout: layout::Layout,
    render: Render,
    milliseconds: f64,
    prepare_milliseconds: f64,
}

pub struct App {
    data: Option<Arc<Dataset>>,
    job: Option<Job>,
    query: String,
    source: Source,
    view: Option<View>,
    history: History,
    selected: Option<Id>,
    cache: Option<Cache>,
    depth: u8,
    detail: Detail,
    error: Option<String>,
    notice: Option<String>,
    gpu: String,
    help: bool,
    tree: Tree,
    tree_scroll: Option<usize>,
    shell_result: Option<Receiver<Result<(), String>>>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, source: Source) -> Self {
        chrome::set_style(&cc.egui_ctx);
        let gpu = cc
            .wgpu_render_state
            .as_ref()
            .map(|state| {
                let info = state.adapter.get_info();
                format!("{} / {:?}", info.name, info.backend)
            })
            .unwrap_or_else(|| "GPU renderer".into());
        let query = match &source {
            Source::Everything(q) => q.clone(),
            _ => String::new(),
        };
        let mut app = Self {
            data: None,
            job: None,
            query,
            source: source.clone(),
            view: None,
            history: History::default(),
            selected: None,
            cache: None,
            depth: 32,
            detail: Detail::Fine,
            error: None,
            notice: None,
            gpu,
            help: false,
            tree: Tree::default(),
            tree_scroll: None,
            shell_result: None,
        };
        app.start(&cc.egui_ctx, source);
        app
    }
    fn start(&mut self, ctx: &egui::Context, source: Source) {
        if self.job.is_some() {
            return;
        }
        self.source = source.clone();
        self.error = None;
        self.notice = None;
        let (tx, rx) = mpsc::sync_channel(8);
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let worker_ctx = ctx.clone();
        let thread = std::thread::spawn(move || {
            let progress = |stage: &str, done, total| {
                let _ = tx.try_send(Event::Progress(stage.to_owned(), done, total));
                worker_ctx.request_repaint();
            };
            let result = match source {
                Source::Everything(q) => transfer::load(&q, &worker_cancel, progress),
                Source::Sample(n, flat) => model::synthetic(n, flat, &worker_cancel, progress),
            };
            send_result(tx, result, &worker_ctx);
        });
        self.job = Some(Job {
            rx,
            cancel,
            stage: "Starting".into(),
            done: 0,
            total: 0,
            since: Instant::now(),
            thread: Some(thread),
        });
    }
    fn receive(&mut self) {
        let mut finished = None;
        if let Some(job) = &mut self.job {
            loop {
                match job.rx.try_recv() {
                    Ok(Event::Progress(stage, done, total)) => {
                        job.stage = stage;
                        job.done = done;
                        job.total = total;
                    }
                    Ok(Event::Done(result)) => {
                        finished = Some(result);
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        finished = Some(Err("The loader stopped unexpectedly.".into()));
                        break;
                    }
                }
            }
        }
        if let Some(result) = finished {
            self.job = None;
            match result {
                Ok(data) => {
                    let view = View::folder(&data, ROOT);
                    self.tree = Tree::new(&data);
                    self.tree_scroll = Some(0);
                    self.notice = if data.capture_note.is_empty() {
                        None
                    } else {
                        Some(data.capture_note.clone())
                    };
                    self.data = Some(Arc::new(*data));
                    self.view = Some(view);
                    self.history = History::default();
                    self.selected = None;
                    self.cache = None;
                }
                Err(error) if error == "Cancelled" => {
                    self.notice =
                        Some("Loading cancelled. The previous view is still available.".into())
                }
                Err(error) => self.error = Some(error),
            }
        }
    }
    fn navigate(&mut self, target: View) {
        if self.view == Some(target) {
            return;
        }
        if let Some(current) = self.current_visit() {
            self.history.record(current);
        }
        self.view = Some(target);
        self.selected = None;
        self.cache = None;
        if let Some(data) = &self.data {
            self.selected = if target.parent == ROOT {
                None
            } else {
                Some(target.parent)
            };
            self.tree_scroll = self.tree.reveal(data, target.parent);
        }
    }
    fn current_visit(&self) -> Option<Visit> {
        self.view.map(|view| Visit {
            view,
            selected: self.selected,
        })
    }
    fn restore_visit(&mut self, visit: Visit) {
        self.view = Some(visit.view);
        self.selected = visit.selected;
        if let Some(data) = &self.data {
            self.tree_scroll = self
                .tree
                .reveal(data, visit.selected.unwrap_or(visit.view.parent));
        }
        self.cache = None;
    }
    fn back(&mut self) {
        if let Some(current) = self.current_visit()
            && let Some(visit) = self.history.back(current)
        {
            self.restore_visit(visit);
        }
    }
    fn forward(&mut self) {
        if let Some(current) = self.current_visit()
            && let Some(visit) = self.history.forward(current)
        {
            self.restore_visit(visit);
        }
    }
    fn canvas(&mut self, ui: &mut egui::Ui, data: &Dataset, view: View) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if ui
                .add_enabled(
                    self.history.can_back(),
                    IconButton::new(Icon::Back, "").quiet(),
                )
                .on_hover_text("Back \u{00b7} Mouse 4 \u{00b7} Alt+Left \u{00b7} Backspace")
                .clicked()
            {
                self.back();
            }
            if ui
                .add_enabled(
                    self.history.can_forward(),
                    IconButton::new(Icon::Forward, "").quiet(),
                )
                .on_hover_text(
                    "Forward \u{00b7} Mouse 5 \u{00b7} Alt+Right \u{00b7} Shift+Backspace",
                )
                .clicked()
            {
                self.forward();
            }
            ui.add_space(8.0);
            let view = self.view.unwrap_or(view);
            egui::ScrollArea::horizontal()
                .id_salt("breadcrumbs")
                .auto_shrink([false, false])
                .max_height(32.0)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .stick_to_right(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for id in data.ancestors(view.parent) {
                            if id != ROOT {
                                let (rect, _) =
                                    ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                                Icon::ChevronRight.paint(ui.painter(), rect, MUTED);
                            }
                            let label = if id == ROOT {
                                "All files"
                            } else {
                                data.name(id)
                            };
                            let color = if id == view.parent { TEXT } else { MUTED };
                            let response = ui.add(
                                egui::Button::new(RichText::new(label).color(color)).frame(false),
                            );
                            tooltip::show(&response, |ui| tooltip::path(ui, &data.path(id)));
                            if response.clicked() {
                                self.navigate(View::folder(data, id));
                            }
                        }
                        if view.is_group(data) {
                            ui.label(RichText::new("/ grouped entries").color(ACCENT));
                        }
                    });
                });
        });
        let view = self.view.unwrap_or(view);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format_bytes(view.bytes(data))).size(26.0).strong());
            ui.label(RichText::new("Logical size").size(12.0).color(MUTED));
            ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
                ui.allocate_ui_with_layout(vec2(360.0, 32.0), UiLayout::left_to_right(Align::Center), |ui| {
                    ui.label(RichText::new("Detail").size(12.0).color(MUTED));
                    egui::ComboBox::from_id_salt("treemap-detail").selected_text(self.detail.label()).width(85.0)
                        .show_ui(ui, |ui| {
                            for detail in [Detail::Balanced, Detail::Fine, Detail::Pixel] { ui.selectable_value(&mut self.detail, detail, detail.label()); }
                        }).response.on_hover_text("Balanced: simpler view. Fine: more small files. Pixel: maximum detail.");
                    ui.add_space(10.0);
                    ui.label(RichText::new("Depth").size(12.0).color(MUTED));
                    ui.spacing_mut().slider_width = 100.0;
                    ui.add(egui::Slider::new(&mut self.depth, 1..=64).logarithmic(true).show_value(true))
                        .on_hover_text("Show more nested folders. Double-click a folder to explore it.");
                });
            });
        });
        if !data.query.is_empty() {
            ui.label(
                RichText::new(format!("Filtered snapshot: {}", data.query))
                    .size(12.0)
                    .color(ACCENT),
            );
        }
        if data.unknown_sizes > 0 {
            ui.label(
                RichText::new(format!(
                    "{} {}; totals are incomplete.",
                    format_count(data.unknown_sizes as usize),
                    if data.unknown_sizes == 1 {
                        "file has an unknown size"
                    } else {
                        "files have unknown sizes"
                    }
                ))
                .color(ACCENT),
            );
        }
        ui.add_space(12.0);
        let size = vec2(
            ui.available_width(),
            (ui.available_height() - 28.0).max(40.0),
        );
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let changed = self.cache.as_ref().is_none_or(|c| {
            c.view != view
                || c.rect != rect
                || c.depth != self.depth
                || c.detail != self.detail
                || c.pixels_per_point != ui.ctx().pixels_per_point()
        });
        if changed {
            let start = Instant::now();
            let layout = layout::build(data, view, rect, self.detail.settings(self.depth));
            let milliseconds = start.elapsed().as_secs_f64() * 1000.0;
            let start = Instant::now();
            let render = Render::prepare(&ui.painter().with_clip_rect(rect), data, &layout);
            self.cache = Some(Cache {
                view,
                rect,
                depth: self.depth,
                detail: self.detail,
                pixels_per_point: ui.ctx().pixels_per_point(),
                layout,
                render,
                milliseconds,
                prepare_milliseconds: start.elapsed().as_secs_f64() * 1000.0,
            });
        }
        let cache = self.cache.as_ref().unwrap();
        let pointer = response.hover_pos();
        let hovered =
            pointer.and_then(|p| cache.layout.tiles.iter().rposition(|t| t.rect.contains(p)));
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 4.0, BG);
        cache.render.paint(&painter);
        let selected_tile = self.selected.and_then(|selected| {
            cache
                .layout
                .tiles
                .iter()
                .position(|t| matches!(t.kind, TileKind::Entry(id) if id == selected))
        });
        for (index, width) in [(selected_tile, 2.0), (hovered, 1.0)] {
            if let Some(index) = index {
                painter.rect_stroke(
                    cache.layout.tiles[index].rect,
                    0.0,
                    Stroke::new(width, TEXT),
                    StrokeKind::Inside,
                );
            }
        }
        if cache.layout.tiles.is_empty() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "No known nonzero file sizes in this view",
                FontId::proportional(18.0),
                MUTED,
            );
        }
        let mut target = None;
        let mut select = None;
        let hovered_kind = hovered.map(|index| cache.layout.tiles[index].kind);
        if let Some(index) = hovered {
            let tile = cache.layout.tiles[index];
            tooltip::show(&response, |ui| {
                match tile.kind {
                    TileKind::Entry(id) => {
                        tooltip::entry(ui, data, id);
                        if data.node(id).is_dir() {
                            ui.small(format!(
                                "{} files",
                                format_count(data.node(id).file_count as usize)
                            ));
                        }
                    }
                    TileKind::Group(group) => {
                        ui.strong(format!(
                            "{} grouped entries",
                            format_count(group.end - group.start)
                        ));
                        ui.small("Double-click to reveal smaller entries.");
                    }
                }
                ui.label(format!(
                    "{} · {:.2}% of this view",
                    format_bytes(tile.bytes),
                    tile.bytes as f64 / view.bytes(data).max(1) as f64 * 100.0
                ));
            });
            if response.clicked()
                && let TileKind::Entry(id) = tile.kind
            {
                select = Some(id);
            }
            if response.double_clicked() {
                target = match tile.kind {
                    TileKind::Entry(id) if data.node(id).is_dir() => Some(View::folder(data, id)),
                    TileKind::Group(group) => Some(group),
                    _ => None,
                };
            }
        }
        ui.add_space(5.0);
        ui.horizontal_wrapped(|ui| {
            for (label, color) in treemap::LEGEND {
                let (swatch, _) = ui.allocate_exact_size(vec2(6.0, 6.0), Sense::hover());
                ui.painter().rect_filled(swatch, 1.0, color);
                ui.label(RichText::new(label).size(11.0).color(MUTED));
            }
        });
        if let Some(target) = target {
            self.navigate(target);
        } else if let Some(id) = select {
            self.select_entry(data, id, true);
        }
        if response.secondary_clicked()
            && let Some(TileKind::Entry(id)) = hovered_kind
        {
            self.select_entry(data, id, true);
        }
        response.context_menu(|ui| {
            if let Some(id) = self.selected {
                self.item_menu(ui, data, id);
            } else {
                ui.label("Select a file or folder for actions.");
            }
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive();
        if let Some(result) = &self.shell_result
            && let Ok(result) = result.try_recv()
        {
            self.shell_result = None;
            match result {
                Ok(()) => self.notice = Some("Opened in Explorer.".into()),
                Err(error) => self.error = Some(error),
            }
        }
        match navigation::input(ctx) {
            Some(Direction::Back) => self.back(),
            Some(Direction::Forward) => self.forward(),
            None => {}
        }
        if !ctx.wants_keyboard_input()
            && ctx.input(|i| i.key_pressed(egui::Key::F5))
            && self.job.is_none()
        {
            self.start(ctx, self.source.clone());
        }
        self.toolbar(ctx);
        self.status(ctx);
        if let (Some(data), Some(view)) = (self.data.clone(), self.view) {
            self.filesystem(ctx, &data);
            let view = self.view.unwrap_or(view);
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(BG)
                        .inner_margin(egui::Margin::symmetric(20, 12)),
                )
                .show(ctx, |ui| self.canvas(ui, &data, view));
        } else {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(BG))
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space((ui.available_height() * 0.3).max(20.0));
                        ui.heading("Your files, in perspective.");
                        ui.add_space(12.0);
                        ui.label("Keep Everything running to explore your indexed files.");
                        ui.add_space(12.0);
                        if self.job.is_some() {
                            ui.spinner();
                            ui.label("Preparing your first view…");
                        } else if ui.button("Load index").clicked() {
                            self.start(ctx, Source::Everything(self.query.clone()));
                        }
                    });
                });
        }
        if self.help {
            egui::Window::new("About everytree").open(&mut self.help).resizable(false).default_width(470.0).show(ctx, |ui| {
                ui.heading(concat!("everytree ", env!("CARGO_PKG_VERSION")));
                ui.label("A disk space explorer powered by Everything.");
                ui.separator();
                ui.label("Click a treemap entry to reveal it in the filesystem tree and inspect its location. Use the tree arrows to expand folders. Double-click a map folder to zoom. Right-click for Explorer and path actions. Mouse 4/5 or Alt+Left/Right navigate back/forward. Backspace goes back; Shift+Backspace goes forward. F5 reloads.");
                ui.label("Logical sizes count indexed file lengths. They are not physical disk allocation or guaranteed reclaimable space. Hard links may be counted more than once. Empty folders are omitted.");
                ui.label("This is a snapshot of Everything's index. Folder totals include only matching files. Refresh after moving, adding, or removing files to update the view.");
                ui.label("Unknown sizes remain in the contents list and are excluded from the treemap's area. The app never deletes files.");
                ui.collapsing("Diagnostics", |ui| {
                    ui.small(format!("Renderer: {}", self.gpu));
                    if let Some(data) = &self.data {
                        ui.small(format!("Source: {}", data.source));
                        ui.small(format!("Load: {:.2}s · data buffers: {}", data.elapsed.as_secs_f64(), format_bytes(data.storage_bytes() as u64)));
                        if data.load_stats.queries > 0 {
                            ui.small(format!("SDK: {} queries / {:.2}s · hierarchy: {:.2}s · finalize: {:.2}s", data.load_stats.queries, data.load_stats.query_us as f64 / 1e6, data.load_stats.decode_us as f64 / 1e6, data.load_stats.finalize_us as f64 / 1e6));
                        }
                    }
                    if let Some(cache) = &self.cache {
                        ui.small(format!("{} tiles · {} individual files · {} groups", format_count(cache.layout.tiles.len()), format_count(cache.render.files), format_count(cache.render.groups)));
                        ui.small(format!("Layout: {:.2} ms · mesh/text preparation: {:.2} ms", cache.milliseconds, cache.prepare_milliseconds));
                    }
                    ui.small("Timings exclude GPU execution. Data buffers exclude Everything, temporary SDK results, graphics memory, and other process allocations.");
                });
            });
        }
    }
}
impl Drop for App {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }
}
fn send_result(tx: SyncSender<Event>, result: Result<Dataset, String>, ctx: &egui::Context) {
    let _ = tx.send(Event::Done(result.map(Box::new)));
    ctx.request_repaint();
}
