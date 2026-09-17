use super::*;

pub(super) fn set_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = PANEL;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = BG;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.selection.bg_fill = Color32::from_rgb(72, 60, 46);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, MUTED);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widget.corner_radius = 5.into();
        widget.bg_stroke = Stroke::new(1.0_f32, BORDER);
    }
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(36, 40, 43);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(47, 52, 55);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(85, 92, 94));
    style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(59, 53, 44);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    style.spacing.item_spacing = vec2(8.0, 8.0);
    style.spacing.button_padding = vec2(10.0, 6.0);
    style.spacing.interact_size.y = 28.0;
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(13.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(11.0));
    ctx.set_style(style);
}

/// Footer columns are determined by window width, never by the current stage or digits.
struct StatusSlots {
    indicator: Rect,
    stage: Rect,
    progress: Rect,
    count: Rect,
    elapsed: Rect,
    cancel: Rect,
}
impl StatusSlots {
    fn new(row: Rect) -> Self {
        let slot = |right: f32, width: f32| {
            Rect::from_min_size(pos2(right - width, row.top()), vec2(width, row.height()))
        };
        let cancel = slot(row.right(), 86.0);
        let elapsed = slot(cancel.left() - 12.0, 64.0);
        let count = slot(elapsed.left() - 16.0, 194.0);
        let progress = slot(count.left() - 16.0, 138.0);
        let indicator =
            Rect::from_center_size(pos2(row.left() + 7.0, row.center().y), vec2(14.0, 14.0));
        let stage = Rect::from_min_max(
            pos2(row.left() + 26.0, row.top()),
            pos2(progress.left() - 24.0, row.bottom()),
        );
        Self {
            indicator,
            stage,
            progress,
            count,
            elapsed,
            cancel,
        }
    }
}

fn fixed_text(ui: &mut egui::Ui, rect: Rect, text: &str, color: Color32, right: bool, mono: bool) {
    let font = if mono {
        FontId::monospace(11.0)
    } else {
        FontId::proportional(12.0)
    };
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap.max_width = rect.width();
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.painter().layout_job(job);
    let x = if right {
        rect.right() - galley.size().x
    } else {
        rect.left()
    };
    ui.painter().with_clip_rect(rect).galley(
        pos2(x, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    ui.interact(
        rect,
        ui.id().with(("status-text", rect.left().to_bits())),
        Sense::hover(),
    )
    .on_hover_text(text);
}

impl App {
    pub(super) fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar")
            .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::symmetric(20, 14)))
            .show(ctx, |ui| {
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
                Icon::Map.paint(ui.painter(), Rect::from_center_size(pos2(row.left() + 12.0, row.center().y), vec2(24.0, 24.0)), ACCENT);
                ui.painter().text(pos2(row.left() + 36.0, row.center().y), Align2::LEFT_CENTER, "everytree", FontId::proportional(20.0), TEXT);
                let about = Rect::from_min_size(pos2(row.right() - 88.0, row.top()), vec2(88.0, 36.0));
                let load = about.translate(vec2(-122.0, 0.0)).with_max_x(about.left() - 12.0);
                let search = Rect::from_min_max(pos2(row.left() + 160.0, row.top()), pos2(load.left() - 12.0, row.bottom()));
                let refresh = self.data.as_ref().is_some_and(|data| !data.synthetic && data.query == self.query.trim());
                let button = if self.job.is_some() { "Loading" } else if refresh { "Refresh" } else { "Load index" };
                let load_clicked = ui.scope_builder(egui::UiBuilder::new().max_rect(load), |ui| {
                    ui.add_enabled_ui(self.job.is_none(), |ui| ui.place(load, IconButton::new(Icon::Refresh, button).primary().width(load.width()))).inner
                        .on_hover_text(if refresh { "Refresh this snapshot · F5" } else { "Load files matching the Everything query · Enter" }).clicked()
                }).inner;
                if ui.place(about, IconButton::new(Icon::Info, "About").quiet()).clicked() { self.help = true; }
                ui.painter().rect_filled(search, 5, PANEL);
                ui.painter().rect_stroke(search, 5, Stroke::new(1.0_f32, BORDER), StrokeKind::Inside);
                Icon::Search.paint(ui.painter(), Rect::from_center_size(pos2(search.left() + 18.0, search.center().y), vec2(16.0, 16.0)), MUTED);
                let text_rect = Rect::from_min_max(search.min + vec2(36.0, 9.0), search.max - vec2(10.0, 9.0));
                let search_ui = ui.place(text_rect, egui::TextEdit::singleline(&mut self.query).frame(false).margin(0).hint_text("Search Everything \u{00b7} all indexed files"))
                    .on_hover_text("Everything search syntax: C:\\Users\\, ext:mp4, or any search query. Leave blank for all indexed files.");
                if search_ui.has_focus() {
                    ui.painter().rect_stroke(search, 5, Stroke::new(1.0_f32, ACCENT), StrokeKind::Inside);
                }
                let enter = search_ui.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (enter || load_clicked) && self.job.is_none() { self.start(ctx, Source::Everything(self.query.clone())); }
            });
    }

    pub(super) fn status(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(20, 6)),
            )
            .show(ctx, |ui| {
                let (row, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
                let slots = StatusSlots::new(row);
                if let Some(job) = &self.job {
                    let cancelling = job.cancel.load(Ordering::Relaxed);
                    ui.place(
                        slots.indicator,
                        egui::Spinner::new().size(14.0).color(ACCENT),
                    );
                    fixed_text(
                        ui,
                        slots.stage,
                        if cancelling {
                            "Cancelling import…"
                        } else {
                            &job.stage
                        },
                        TEXT,
                        false,
                        false,
                    );
                    let track = Rect::from_center_size(
                        slots.progress.center(),
                        vec2(slots.progress.width(), 4.0),
                    );
                    ui.painter().rect_filled(track, 2, BORDER);
                    if job.total > 0 {
                        let fraction = (job.done as f32 / job.total as f32).clamp(0.0, 1.0);
                        if fraction > 0.0 {
                            ui.painter().rect_filled(
                                Rect::from_min_size(
                                    track.min,
                                    vec2(track.width() * fraction, track.height()),
                                ),
                                2,
                                ACCENT,
                            );
                        }
                        fixed_text(
                            ui,
                            slots.count,
                            &format!("{} / {}", format_count(job.done), format_count(job.total)),
                            MUTED,
                            true,
                            true,
                        );
                        ui.interact(
                            slots.progress,
                            ui.id().with("import-progress"),
                            Sense::hover(),
                        )
                        .on_hover_text(format!("{:.0}% of the current stage", fraction * 100.0));
                    } else {
                        let t = ui.input(|i| i.time) as f32;
                        let x = (t * 2.0).sin() * 0.5 + 0.5;
                        let marker = Rect::from_min_size(
                            track.min + vec2(x * (track.width() - 28.0), 0.0),
                            vec2(28.0, 4.0),
                        );
                        ui.painter().rect_filled(marker, 2, ACCENT);
                        fixed_text(
                            ui,
                            slots.count,
                            "Waiting for Everything",
                            MUTED,
                            true,
                            false,
                        );
                    }
                    fixed_text(
                        ui,
                        slots.elapsed,
                        &format!("{:.1}s", job.since.elapsed().as_secs_f64()),
                        MUTED,
                        true,
                        true,
                    );
                    let cancel = ui
                        .scope_builder(egui::UiBuilder::new().max_rect(slots.cancel), |ui| {
                            ui.add_enabled(
                                !cancelling,
                                IconButton::new(Icon::Close, "Cancel")
                                    .quiet()
                                    .width(slots.cancel.width()),
                            )
                            .clicked()
                        })
                        .inner;
                    if cancel {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                    ctx.request_repaint_after(Duration::from_millis(100));
                } else {
                    let color = if self.error.is_some() {
                        Color32::from_rgb(237, 151, 130)
                    } else {
                        Color32::from_rgb(125, 184, 157)
                    };
                    ui.painter()
                        .circle_filled(slots.indicator.center(), 3.0, color);
                    let text = if let Some(data) = &self.data {
                        format!(
                            "{} files  ·  {} folders",
                            format_count(data.node(ROOT).file_count as usize),
                            format_count(data.folder_count())
                        )
                    } else if self.error.is_some() {
                        "Import failed".into()
                    } else {
                        "Ready".into()
                    };
                    let left = Rect::from_min_max(
                        slots.stage.min,
                        pos2(row.right() - 230.0, row.bottom()),
                    );
                    fixed_text(ui, left, &text, MUTED, false, false);
                    fixed_text(
                        ui,
                        Rect::from_min_max(pos2(row.right() - 220.0, row.top()), row.max),
                        "Snapshot · F5 to refresh",
                        MUTED,
                        true,
                        false,
                    );
                }
                if let Some((message, is_error)) = self
                    .error
                    .as_ref()
                    .map(|s| (s, true))
                    .or_else(|| self.notice.as_ref().map(|s| (s, false)))
                {
                    let message = message.clone();
                    ui.separator();
                    ui.horizontal(|ui| {
                        let text_width = (ui.available_width() - 44.0).max(100.0);
                        ui.add_sized(
                            vec2(text_width, 20.0),
                            egui::Label::new(RichText::new(message).size(12.0).color(
                                if is_error {
                                    Color32::from_rgb(245, 150, 130)
                                } else {
                                    MUTED
                                },
                            ))
                            .wrap(),
                        );
                        if ui
                            .add(IconButton::new(Icon::Close, "").quiet())
                            .on_hover_text("Dismiss message")
                            .clicked()
                        {
                            if is_error {
                                self.error = None;
                            } else {
                                self.notice = None;
                            }
                        }
                    });
                }
            });
    }
}
