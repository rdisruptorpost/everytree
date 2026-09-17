//! Small, resolution-independent line icons, painted with the same GPU paths as egui.
//! Keeping these as vectors avoids texture uploads and an SVG decoder at startup.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Back,
    Forward,
    Refresh,
    Info,
    Search,
    Folder,
    File,
    Explorer,
    Copy,
    Locate,
    Collapse,
    Close,
    ChevronRight,
    ChevronDown,
    Map,
}

impl Icon {
    pub fn paint(self, painter: &egui::Painter, rect: Rect, color: Color32) {
        let p = |x: f32, y: f32| rect.min + vec2(x, y) * (rect.width() / 24.0);
        let stroke = Stroke::new(1.6 * rect.width() / 24.0, color);
        let line = |points: &[[f32; 2]]| {
            painter.add(egui::Shape::line(
                points.iter().map(|v| p(v[0], v[1])).collect(),
                stroke,
            ));
        };
        let circle = |x, y, r| {
            painter.circle_stroke(p(x, y), r * rect.width() / 24.0, stroke);
        };
        match self {
            Self::Back => {
                line(&[[14., 5.], [7., 12.], [14., 19.]]);
                line(&[[7., 12.], [21., 12.]]);
            }
            Self::Forward => {
                line(&[[10., 5.], [17., 12.], [10., 19.]]);
                line(&[[3., 12.], [17., 12.]]);
            }
            Self::ChevronRight => line(&[[9., 6.], [15., 12.], [9., 18.]]),
            Self::ChevronDown => line(&[[6., 9.], [12., 15.], [18., 9.]]),
            Self::Refresh => {
                let points: Vec<_> = (0..=24)
                    .map(|i| {
                        let a = (45.0 + i as f32 * 285.0 / 24.0).to_radians();
                        p(12. + 8. * a.cos(), 12. + 8. * a.sin())
                    })
                    .collect();
                painter.add(egui::Shape::line(points, stroke));
                line(&[[19., 3.], [19., 8.], [14., 8.]]);
            }
            Self::Info => {
                circle(12., 12., 9.);
                line(&[[12., 11.], [12., 17.]]);
                painter.circle_filled(p(12., 7.), rect.width() / 24.0, color);
            }
            Self::Search => {
                circle(10., 10., 6.);
                line(&[[14.5, 14.5], [21., 21.]]);
            }
            Self::Folder => line(&[
                [3., 19.],
                [3., 5.],
                [9., 5.],
                [12., 8.],
                [21., 8.],
                [21., 19.],
                [3., 19.],
            ]),
            Self::File => {
                line(&[
                    [5., 3.],
                    [14., 3.],
                    [19., 8.],
                    [19., 21.],
                    [5., 21.],
                    [5., 3.],
                ]);
                line(&[[14., 3.], [14., 8.], [19., 8.]]);
            }
            Self::Explorer => {
                line(&[[11., 5.], [4., 5.], [4., 20.], [19., 20.], [19., 13.]]);
                line(&[[14., 3.], [21., 3.], [21., 10.]]);
                line(&[[21., 3.], [11., 13.]]);
            }
            Self::Copy => {
                line(&[[8., 8.], [20., 8.], [20., 21.], [8., 21.], [8., 8.]]);
                line(&[[16., 8.], [16., 3.], [3., 3.], [3., 16.], [8., 16.]]);
            }
            Self::Locate => {
                circle(12., 12., 6.);
                circle(12., 12., 2.);
                for points in [
                    [[12., 2.], [12., 6.]],
                    [[12., 18.], [12., 22.]],
                    [[2., 12.], [6., 12.]],
                    [[18., 12.], [22., 12.]],
                ] {
                    line(&points);
                }
            }
            Self::Collapse => {
                line(&[[6., 3.], [12., 9.], [18., 3.]]);
                line(&[[6., 21.], [12., 15.], [18., 21.]]);
            }
            Self::Close => {
                line(&[[6., 6.], [18., 18.]]);
                line(&[[18., 6.], [6., 18.]]);
            }
            Self::Map => {
                for (min, size, tint) in [
                    ([2., 2.], [11., 20.], ACCENT),
                    ([15., 2.], [7., 9.], Color32::from_rgb(106, 163, 166)),
                    ([15., 13.], [7., 9.], Color32::from_rgb(142, 134, 177)),
                ] {
                    painter.rect_filled(
                        Rect::from_min_size(
                            p(min[0], min[1]),
                            vec2(size[0], size[1]) * rect.width() / 24.0,
                        ),
                        1.5,
                        tint,
                    );
                }
            }
        }
    }
}

pub(super) struct IconButton<'a> {
    icon: Icon,
    label: &'a str,
    quiet: bool,
    primary: bool,
    width: f32,
}
impl<'a> IconButton<'a> {
    pub fn new(icon: Icon, label: &'a str) -> Self {
        Self {
            icon,
            label,
            quiet: false,
            primary: false,
            width: 0.0,
        }
    }
    pub fn quiet(mut self) -> Self {
        self.quiet = true;
        self
    }
    pub fn primary(mut self) -> Self {
        self.primary = true;
        self
    }
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
}
impl egui::Widget for IconButton<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let id = egui::Id::new("button-icon");
        let mut atoms = egui::Atoms::new(egui::Atom::custom(id, vec2(16.0, 16.0)));
        if !self.label.is_empty() {
            atoms.push_right(self.label);
        }
        let mut button = egui::Button::new(atoms)
            .min_size(vec2(self.width, 32.0))
            .frame_when_inactive(!self.quiet);
        if self.primary {
            button = button
                .fill(Color32::from_rgb(75, 59, 43))
                .stroke(Stroke::new(1.0, Color32::from_rgb(111, 82, 52)));
        }
        let response = button.atom_ui(ui);
        if let Some(rect) = response.rect(id) {
            self.icon.paint(
                ui.painter(),
                rect,
                ui.style().interact(&response.response).text_color(),
            );
        }
        if self.label.is_empty() {
            response.response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    ui.is_enabled(),
                    match self.icon {
                        Icon::Back => "Back",
                        Icon::Forward => "Forward",
                        Icon::Close => "Dismiss",
                        _ => "Action",
                    },
                )
            });
        }
        response.response
    }
}
