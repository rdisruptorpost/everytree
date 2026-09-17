use super::*;

impl App {
    pub(super) fn select_entry(&mut self, data: &Dataset, id: Id, scroll: bool) {
        self.selected = Some(id);
        if scroll {
            self.tree_scroll = self.tree.reveal(data, id);
        }
    }
    fn explorer(&mut self, ctx: &egui::Context, data: &Dataset, id: Id, open: bool) {
        if data.synthetic || id == ROOT || self.shell_result.is_some() {
            return;
        }
        let path = if open && !data.node(id).is_dir() {
            data.path(data.node(id).parent)
        } else {
            data.path(id)
        };
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = if open {
                everytree::shell::open_folder(&path)
            } else {
                everytree::shell::reveal(&path)
            };
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.shell_result = Some(rx);
    }
    pub(super) fn item_menu(&mut self, ui: &mut egui::Ui, data: &Dataset, id: Id) {
        ui.strong(data.name(id));
        ui.separator();
        if ui
            .add_enabled(
                !data.synthetic && self.shell_result.is_none(),
                egui::Button::new("Show in Explorer"),
            )
            .clicked()
        {
            self.explorer(ui.ctx(), data, id, false);
            ui.close();
        }
        if ui
            .add_enabled(
                !data.synthetic && self.shell_result.is_none(),
                egui::Button::new(if data.node(id).is_dir() {
                    "Open folder in Explorer"
                } else {
                    "Open containing folder"
                }),
            )
            .clicked()
        {
            self.explorer(ui.ctx(), data, id, true);
            ui.close();
        }
        if ui.button("Copy full path").clicked() {
            ui.ctx().copy_text(data.path(id));
            ui.close();
        }
        if ui.button("Locate in filesystem tree").clicked() {
            self.select_entry(data, id, true);
            ui.close();
        }
        if ui
            .button(if data.node(id).is_dir() {
                "Zoom treemap to folder"
            } else {
                "Show sibling files in treemap"
            })
            .clicked()
        {
            self.navigate(View::folder(
                data,
                if data.node(id).is_dir() {
                    id
                } else {
                    data.node(id).parent
                },
            ));
            self.select_entry(data, id, true);
            ui.close();
        }
    }
    pub(super) fn filesystem(&mut self, ctx: &egui::Context, data: &Dataset) {
        egui::TopBottomPanel::top("filesystem")
            .resizable(true)
            .default_height((ctx.content_rect().height() * 0.31).clamp(210.0, 282.0))
            .min_height(170.0)
            .max_height(500.0)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(20, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Files & folders").strong());
                    if ui.available_width() > 1100.0 {
                        ui.label(
                            RichText::new("Largest first \u{00b7} right-click for actions")
                                .size(12.0)
                                .color(MUTED),
                        );
                    }
                    ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(IconButton::new(Icon::Collapse, "Collapse all").quiet())
                            .clicked()
                        {
                            self.tree = Tree::new(data);
                            self.tree_scroll = Some(0);
                        }
                        if ui
                            .add_enabled(
                                self.selected.is_some(),
                                IconButton::new(Icon::Locate, "Locate selection").quiet(),
                            )
                            .on_hover_text("Expand the tree to the selected file or folder")
                            .clicked()
                            && let Some(id) = self.selected
                        {
                            self.tree_scroll = self.tree.reveal(data, id);
                        }
                    });
                });
                ui.add_space(4.0);
                let height = ui.available_height();
                let inspector_width = (ui.available_width() * 0.29).clamp(285.0, 460.0);
                let table_width = (ui.available_width() - inspector_width - 24.0).max(300.0);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        vec2(table_width, height),
                        UiLayout::top_down(Align::Min),
                        |ui| self.file_table(ui, data),
                    );
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        vec2(inspector_width, height),
                        UiLayout::top_down(Align::Min),
                        |ui| self.inspector(ui, data),
                    );
                });
            });
    }
    fn file_table(&mut self, ui: &mut egui::Ui, data: &Dataset) {
        let width = (ui.available_width() - 12.0).max(280.0);
        let size_x = width - 210.0;
        let share_x = width - 105.0;
        let files_x = width - 8.0;
        let (header, _) = ui.allocate_exact_size(vec2(width, 20.0), Sense::hover());
        let p = ui.painter();
        for (label, x, align) in [
            ("Name", 44.0, Align2::LEFT_CENTER),
            ("Logical size", size_x, Align2::RIGHT_CENTER),
            ("% of parent", share_x, Align2::RIGHT_CENTER),
            ("Files", files_x, Align2::RIGHT_CENTER),
        ] {
            p.text(
                pos2(header.left() + x, header.center().y),
                align,
                label,
                FontId::proportional(11.0),
                MUTED,
            );
        }
        p.line_segment(
            [header.left_bottom(), header.right_bottom()],
            Stroke::new(1.0, Color32::from_rgb(49, 54, 56)),
        );
        let mut scroll = egui::ScrollArea::vertical()
            .id_salt("file-tree-rows")
            .auto_shrink([false, false]);
        if let Some(row) = self.tree_scroll.take() {
            scroll = scroll.vertical_scroll_offset(row as f32 * 26.0);
        }
        let mut toggle = None;
        let mut select = None;
        let mut zoom = None;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            scroll.show_rows(ui, 26.0, self.tree.len, |ui, range| {
                for row in range {
                    let Some((id, depth)) = self.tree.row(data, row) else {
                        continue;
                    };
                    let n = data.node(id);
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
                    let selected = self.selected == Some(id);
                    if selected || response.hovered() || row % 2 == 0 {
                        ui.painter().rect_filled(
                            rect,
                            2.0,
                            if selected {
                                Color32::from_rgb(67, 61, 49)
                            } else if response.hovered() {
                                Color32::from_rgb(42, 47, 49)
                            } else {
                                Color32::from_rgb(29, 33, 35)
                            },
                        );
                    }
                    let indent = (depth as f32 * 16.0).min((size_x - 240.0).max(0.0));
                    let arrow_rect =
                        Rect::from_min_size(rect.min + vec2(indent, 1.0), vec2(22.0, 24.0));
                    let expand =
                        ui.interact(arrow_rect, ui.id().with(("expand", id)), Sense::click());
                    if n.is_dir() {
                        let icon = if self.tree.is_expanded(id) {
                            Icon::ChevronDown
                        } else {
                            Icon::ChevronRight
                        };
                        icon.paint(
                            ui.painter(),
                            Rect::from_center_size(arrow_rect.center(), vec2(12.0, 12.0)),
                            MUTED,
                        );
                        if expand.clicked() {
                            toggle = Some(id);
                        }
                    }
                    let icon_rect =
                        Rect::from_min_size(rect.min + vec2(indent + 25.0, 5.0), vec2(16.0, 16.0));
                    (if n.is_dir() { Icon::Folder } else { Icon::File }).paint(
                        ui.painter(),
                        icon_rect,
                        if n.is_dir() { ACCENT } else { MUTED },
                    );
                    let name_rect = Rect::from_min_max(
                        rect.min + vec2(indent + 47.0, 0.0),
                        pos2(rect.left() + size_x - 110.0, rect.bottom()),
                    );
                    if n.is_dir() {
                        paint_folder_label(ui.painter(), name_rect, data, id, 13.0, TEXT, MUTED);
                    } else {
                        ui.painter().with_clip_rect(name_rect).text(
                            name_rect.left_center(),
                            Align2::LEFT_CENTER,
                            data.name(id),
                            FontId::proportional(13.0),
                            Color32::from_rgb(194, 206, 210),
                        );
                    }
                    let parent_bytes = data.node(n.parent).bytes;
                    let percentage = if parent_bytes == 0 {
                        0.0
                    } else {
                        100.0 * n.bytes as f64 / parent_bytes as f64
                    };
                    let values = [
                        (
                            format!(
                                "{}{}",
                                format_bytes(n.bytes),
                                if n.size_unknown() { " + ?" } else { "" }
                            ),
                            size_x,
                        ),
                        (format!("{percentage:.1}%"), share_x),
                        (format_count(n.file_count as usize), files_x),
                    ];
                    for (value, x) in values {
                        ui.painter().text(
                            pos2(rect.left() + x, rect.center().y),
                            Align2::RIGHT_CENTER,
                            value,
                            FontId::proportional(12.0),
                            if selected { TEXT } else { MUTED },
                        );
                    }
                    if response.clicked() && !expand.clicked() {
                        select = Some(id);
                    }
                    if response.double_clicked() && n.is_dir() {
                        toggle = Some(id);
                        zoom = Some(id);
                    }
                    if response.secondary_clicked() {
                        self.selected = Some(id);
                    }
                    tooltip::show(&response, |ui| tooltip::entry(ui, data, id));
                    response.context_menu(|ui| self.item_menu(ui, data, id));
                }
            });
        });
        if let Some(id) = toggle {
            self.tree.toggle(data, id);
        }
        if let Some(id) = select {
            self.navigate(View::folder(
                data,
                if data.node(id).is_dir() {
                    id
                } else {
                    data.node(id).parent
                },
            ));
            self.select_entry(data, id, false);
            // Keep the tree stationary when selecting a row already in view.
            self.tree_scroll = None;
        }
        if let Some(id) = zoom {
            self.navigate(View::folder(data, id));
            self.select_entry(data, id, false);
            self.tree_scroll = None;
        }
    }
    fn inspector(&mut self, ui: &mut egui::Ui, data: &Dataset) {
        let Some(id) = self.selected else {
            ui.label(RichText::new("Selection").size(12.0).color(MUTED));
            ui.add_space(12.0);
            ui.label("Select a file or folder to see its location and context.");
            return;
        };
        let node = data.node(id);
        let details_height = (ui.available_height() - 82.0).max(30.0);
        egui::ScrollArea::vertical()
            .id_salt("selection-details")
            .max_height(details_height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.add(
                    egui::Label::new(RichText::new(data.name(id)).size(18.0).strong()).truncate(),
                )
                .on_hover_text(data.name(id));
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format_bytes(node.bytes))
                            .color(ACCENT)
                            .strong(),
                    );
                    let kind = if node.is_dir() {
                        "Folder".into()
                    } else {
                        data.name(id)
                            .rsplit_once('.')
                            .map(|(_, ext)| format!("{} file", ext.to_ascii_uppercase()))
                            .unwrap_or("File".into())
                    };
                    ui.label(RichText::new(kind).color(MUTED));
                    if node.is_dir() {
                        ui.label(format!("{} files", format_count(node.file_count as usize)));
                    }
                });
                ui.add(
                    egui::Label::new(RichText::new(data.path(id)).size(12.0))
                        .wrap()
                        .selectable(true),
                )
                .on_hover_text("Full path · select text to copy a portion, or use Copy path below");
                if id != ROOT {
                    let parent = node.parent;
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("In").color(MUTED));
                        if ui.link(data.name(parent)).clicked() {
                            self.navigate(View::folder(data, parent));
                        }
                        ui.label(
                            RichText::new(format!(
                                "· {} sibling entries",
                                format_count(
                                    data.node(parent).child_count.saturating_sub(1) as usize
                                )
                            ))
                            .size(12.0)
                            .color(MUTED),
                        );
                    });
                }
                if node.size_unknown() {
                    ui.label(RichText::new("Some indexed sizes are unknown.").color(ACCENT));
                }
            });
        ui.add_space(4.0);
        let button_width = (ui.available_width() - 8.0) / 2.0;
        egui::Grid::new("selection-actions")
            .spacing(vec2(8.0, 6.0))
            .show(ui, |ui| {
                let shell_enabled = !data.synthetic && id != ROOT && self.shell_result.is_none();
                if ui
                    .add_enabled(
                        shell_enabled,
                        IconButton::new(
                            Icon::Explorer,
                            if self.shell_result.is_some() {
                                "Opening…"
                            } else {
                                "Explorer"
                            },
                        )
                        .width(button_width),
                    )
                    .on_hover_text("Show this entry in Windows Explorer")
                    .clicked()
                {
                    self.explorer(ui.ctx(), data, id, false);
                }
                if ui
                    .add(IconButton::new(Icon::Copy, "Copy path").width(button_width))
                    .clicked()
                {
                    ui.ctx().copy_text(data.path(id));
                }
                ui.end_row();
                if ui
                    .add_enabled(
                        shell_enabled,
                        IconButton::new(Icon::Folder, "Open folder").width(button_width),
                    )
                    .on_hover_text(if node.is_dir() {
                        "Open this folder in Explorer"
                    } else {
                        "Open the containing folder in Explorer"
                    })
                    .clicked()
                {
                    self.explorer(ui.ctx(), data, id, true);
                }
                if ui
                    .add(IconButton::new(Icon::Map, "Zoom here").width(button_width))
                    .on_hover_text("Explore this folder in the treemap")
                    .clicked()
                {
                    self.navigate(View::folder(
                        data,
                        if node.is_dir() { id } else { node.parent },
                    ));
                    self.select_entry(data, id, true);
                }
                ui.end_row();
            });
    }
}
