use super::*;

pub(super) fn show<R>(
    response: &egui::Response,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<egui::InnerResponse<R>> {
    // A treemap response covers nearly the whole window. Anchor to the cursor,
    // not that rectangle, so the popup can flip to either side at window edges.
    let width = (response.ctx.content_rect().width() - 40.0).clamp(1.0, 420.0);
    egui::Tooltip::for_enabled(response)
        .at_pointer()
        .gap(12.0)
        .width(width)
        .show(|ui| {
            // Reset both bounds each frame: egui Areas otherwise reuse the size
            // of the previous item and can progressively shrink dynamic tooltips.
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 6.0;
            content(ui)
        })
}

pub(super) fn entry(ui: &mut egui::Ui, data: &Dataset, id: Id) {
    ui.add(egui::Label::new(RichText::new(data.name(id)).strong()).wrap());
    path(ui, &data.path(id));
}

pub(super) fn path(ui: &mut egui::Ui, path: &str) {
    // Normal wrapping prefers word/punctuation boundaries and can still split
    // an unusually long filename. Display the original path without modifying it.
    ui.add(egui::Label::new(RichText::new(path).monospace().size(12.0).color(MUTED)).wrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_paths_keep_readable_tooltips_inside_window_edges() {
        for scale in [1.0, 1.5, 2.0] {
            let ctx = egui::Context::default();
            chrome::set_style(&ctx);
            ctx.style_mut(|s| s.interaction.tooltip_delay = 0.0);
            let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(940.0, 600.0));
            let mut frame = 0;
            // Reuse one canvas ID, as moving between tiles does in the app.
            for pointer in [
                pos2(30.0, 30.0),
                pos2(910.0, 30.0),
                pos2(910.0, 570.0),
                pos2(30.0, 570.0),
            ] {
                for path in [
                    "C:\\",
                    "C:\\Users\\c\\Documents\\Claude",
                    "\\\\server\\share\\Projects\\A long folder name with spaces\\Exports\\A_very_long_filename_with_no_spaces_that_should_wrap_without_widening_the_window_012345678901234567890123456789.mp4",
                ] {
                    let mut popup = None;
                    for _ in 0..5 {
                        let mut input = egui::RawInput {
                            screen_rect: Some(screen),
                            time: Some(frame as f64 * 0.25),
                            events: vec![egui::Event::PointerMoved(pointer)],
                            ..Default::default()
                        };
                        input
                            .viewports
                            .get_mut(&egui::ViewportId::ROOT)
                            .unwrap()
                            .native_pixels_per_point = Some(scale);
                        frame += 1;
                        let _ = ctx.run(input, |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                let (_, response) =
                                    ui.allocate_exact_size(ui.available_size(), Sense::hover());
                                popup = show(&response, |ui| {
                                    ui.strong("Claude");
                                    super::path(ui, path);
                                    ui.small("482,408 files");
                                    ui.label("163.7 GiB · 9.09% of this view");
                                })
                                .map(|r| r.response.rect);
                            });
                        });
                    }
                    let popup = popup.expect("tooltip should open after hovering");
                    assert!(
                        popup.width() >= 420.0 && popup.width() <= 450.0,
                        "{popup:?}"
                    );
                    assert!(
                        screen.expand(1.0).contains_rect(popup),
                        "scale {scale}: {popup:?}"
                    );
                    assert!(
                        popup.height() < 230.0,
                        "path was squeezed into a tall column: {popup:?}"
                    );
                }
            }
        }
    }
}
