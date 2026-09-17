//! Cached GPU geometry and text for the visible treemap, never the full index.
use crate::{
    layout::{Layout, TileKind},
    model::{Dataset, Id, format_bytes, format_count},
};
use eframe::egui::{self, Color32, FontId, Mesh, Painter, Rect, Shape, pos2, vec2};
use std::sync::Arc;

pub const LEGEND: [(&str, Color32); 10] = [
    ("Video", Color32::from_rgb(35, 151, 225)),
    ("Audio", Color32::from_rgb(25, 193, 182)),
    ("Images", Color32::from_rgb(88, 183, 63)),
    ("Archives", Color32::from_rgb(229, 141, 28)),
    ("Code", Color32::from_rgb(215, 185, 48)),
    ("Data", Color32::from_rgb(206, 75, 161)),
    ("Documents", Color32::from_rgb(224, 85, 88)),
    ("Apps", Color32::from_rgb(100, 120, 224)),
    ("Disk images", Color32::from_rgb(163, 69, 222)),
    ("3D", Color32::from_rgb(53, 187, 133)),
];

fn hash(text: &str) -> u32 {
    text.bytes().fold(2166136261, |h, b| {
        (h ^ b.to_ascii_lowercase() as u32).wrapping_mul(16777619)
    })
}

pub fn file_color(name: &str) -> Color32 {
    let ext = name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
    // Most extensions fit on the stack. Hash arbitrarily long extensions
    // directly, without allocating a lowercase String for every visible file.
    let mut lower = [0u8; 24];
    let normal = if ext.len() <= lower.len() && ext.is_ascii() {
        for (out, byte) in lower.iter_mut().zip(ext.bytes()) {
            *out = byte.to_ascii_lowercase();
        }
        std::str::from_utf8(&lower[..ext.len()]).unwrap()
    } else {
        ""
    };
    let category = match normal {
        "mp4" | "mov" | "mkv" | "avi" | "webm" | "mxf" | "crm" | "braw" | "r3d" | "m4v" | "mts" => {
            Some(0)
        }
        "wav" | "mp3" | "flac" | "ogg" | "aac" | "aiff" | "m4a" => Some(1),
        "jpg" | "jpeg" | "png" | "webp" | "raw" | "psd" | "heic" | "exr" | "tif" | "tiff"
        | "bmp" | "gif" | "svg" => Some(2),
        "zip" | "7z" | "rar" | "tar" | "gz" | "pak" | "bz2" | "xz" | "zst" | "whl" => Some(3),
        "rs" | "js" | "ts" | "tsx" | "jsx" | "py" | "pyc" | "cpp" | "c" | "h" | "cs" | "html"
        | "css" | "java" | "go" | "nix" | "sh" => Some(4),
        "json" | "csv" | "db" | "sqlite" | "parquet" | "xml" | "yaml" | "toml" | "bin"
        | "safetensors" | "pt" | "onnx" => Some(5),
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "pptx" | "txt" | "md" | "rtf" | "epub" => Some(6),
        "exe" | "dll" | "msi" | "sys" | "so" | "pyd" | "lib" | "pdb" => Some(7),
        "iso" | "vhd" | "vhdx" | "vmdk" | "qcow2" | "img" => Some(8),
        "blend" | "fbx" | "obj" | "abc" | "usd" | "usdc" | "uasset" | "stl" => Some(9),
        _ => None,
    };
    let h = hash(ext);
    if let Some(index) = category {
        let base = egui::ecolor::Hsva::from(LEGEND[index].1);
        Color32::from(egui::ecolor::Hsva::new(
            (base.h + (h % 13) as f32 / 300.0 - 0.02).rem_euclid(1.0),
            (base.s + ((h >> 8) % 7) as f32 * 0.015).min(0.90),
            0.72 + ((h >> 16) % 12) as f32 * 0.018,
            1.0,
        ))
    } else if ext.is_empty() {
        Color32::from_rgb(128, 142, 153)
    } else {
        Color32::from(egui::ecolor::Hsva::new(
            (h % 360) as f32 / 360.0,
            0.58 + ((h >> 9) % 20) as f32 / 100.0,
            0.82,
            1.0,
        ))
    }
}

#[derive(Default)]
pub struct Render {
    pub mesh: Arc<Mesh>,
    labels: Vec<(Rect, Shape)>,
    pub files: usize,
    pub groups: usize,
}
impl Render {
    pub fn prepare(painter: &Painter, data: &Dataset, layout: &Layout) -> Self {
        let mut mesh = Mesh::default();
        mesh.reserve_vertices(layout.tiles.len() * 4);
        let mut render = Self::default();
        for (index, tile) in layout.tiles.iter().enumerate() {
            let has_children = layout
                .tiles
                .get(index + 1)
                .is_some_and(|next| next.depth > tile.depth);
            let (is_dir, color) = match tile.kind {
                TileKind::Entry(id) if data.node(id).is_dir() => {
                    (true, Color32::from_gray(43 + (tile.depth % 3) * 5))
                }
                TileKind::Entry(id) => {
                    render.files += 1;
                    (false, file_color(data.name(id)))
                }
                TileKind::Group(_) => {
                    render.groups += 1;
                    (false, Color32::from_rgb(77, 83, 90))
                }
            };
            let gap = if is_dir {
                0.0
            } else {
                (tile.rect.width().min(tile.rect.height()) * 0.08).min(0.45)
            };
            let rect = tile.rect.shrink(gap);
            if rect.width() <= 0.0 || rect.height() <= 0.0 {
                continue;
            }
            if !is_dir && rect.width().min(rect.height()) >= 5.0 {
                cushion(&mut mesh, rect, color);
            } else {
                mesh.add_colored_rect(rect, color);
            }
            if rect.width() <= 65.0
                || rect.height() <= 18.0
                || (is_dir && has_children && tile.header_height == 0.0)
            {
                continue;
            }
            let clip = Rect::from_min_max(rect.min + vec2(4.0, 0.0), rect.max - vec2(4.0, 0.0));
            match tile.kind {
                TileKind::Entry(id) if is_dir => {
                    let header =
                        Rect::from_min_max(clip.min, pos2(clip.right(), clip.top() + 19.0));
                    folder_labels(
                        &mut render.labels,
                        painter,
                        header,
                        data,
                        id,
                        12.0,
                        (Color32::from_gray(238), Color32::from_gray(207)),
                    );
                }
                _ => {
                    let title = match tile.kind {
                        TileKind::Entry(id) => data.name(id).to_owned(),
                        TileKind::Group(group) => {
                            format!("{} grouped", format_count(group.end - group.start))
                        }
                    };
                    text(
                        &mut render.labels,
                        painter,
                        clip,
                        clip.min + vec2(1.0, 3.0),
                        title,
                        12.0,
                        Color32::WHITE,
                    );
                    if rect.height() > 40.0 {
                        text(
                            &mut render.labels,
                            painter,
                            clip,
                            clip.min + vec2(1.0, 19.0),
                            format_bytes(tile.bytes),
                            11.0,
                            Color32::from_gray(225),
                        );
                    }
                }
            }
        }
        render.mesh = Arc::new(mesh);
        render
    }
    pub fn paint(&self, painter: &Painter) {
        painter.add(Shape::Mesh(self.mesh.clone()));
        for (clip, shape) in &self.labels {
            painter.with_clip_rect(*clip).add(shape.clone());
        }
    }
}

// A small, cached vertex mesh gives larger file tiles a cushion highlight;
// subpixel/tiny tiles use four vertices so detail doesn't vanish into borders.
fn cushion(mesh: &mut Mesh, rect: Rect, color: Color32) {
    let base = mesh.vertices.len() as u32;
    for y in 0..3 {
        for x in 0..3 {
            let brightness = if x == 1 && y == 1 {
                1.13
            } else if x == 1 || y == 1 {
                0.84
            } else {
                0.60
            };
            let shade = Color32::from_rgb(
                (color.r() as f32 * brightness).min(255.0) as u8,
                (color.g() as f32 * brightness).min(255.0) as u8,
                (color.b() as f32 * brightness).min(255.0) as u8,
            );
            mesh.colored_vertex(
                pos2(
                    rect.left() + rect.width() * x as f32 / 2.0,
                    rect.top() + rect.height() * y as f32 / 2.0,
                ),
                shade,
            );
        }
    }
    for y in 0..2 {
        for x in 0..2 {
            let a = base + y * 3 + x;
            mesh.add_triangle(a, a + 1, a + 3);
            mesh.add_triangle(a + 1, a + 4, a + 3);
        }
    }
}

fn text(
    labels: &mut Vec<(Rect, Shape)>,
    painter: &Painter,
    clip: Rect,
    pos: egui::Pos2,
    text: String,
    size: f32,
    color: Color32,
) {
    let galley = painter.layout_no_wrap(text, FontId::proportional(size), color);
    labels.push((clip, Shape::galley(pos, galley, color)));
}

fn folder_labels(
    labels: &mut Vec<(Rect, Shape)>,
    painter: &Painter,
    rect: Rect,
    data: &Dataset,
    id: Id,
    font_size: f32,
    colors: (Color32, Color32),
) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let (name_color, size_color) = colors;
    let node = data.node(id);
    let size = painter.layout_no_wrap(
        format!(
            "{}{}",
            format_bytes(node.bytes),
            if node.size_unknown() { " + ?" } else { "" }
        ),
        FontId::proportional(font_size - 1.0),
        size_color,
    );
    let gap = 8.0;
    let name_width = rect.width() - size.size().x - gap;
    let mut size_x = rect.left();
    if name_width >= 18.0 {
        let mut job = egui::text::LayoutJob::simple(
            data.name(id).to_owned(),
            FontId::proportional(font_size),
            name_color,
            name_width,
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let name = painter.layout_job(job);
        size_x += name.size().x + gap;
        labels.push((
            rect,
            Shape::galley(
                pos2(rect.left(), rect.center().y - name.size().y / 2.0),
                name,
                name_color,
            ),
        ));
    }
    labels.push((
        rect,
        Shape::galley(
            pos2(size_x, rect.center().y - size.size().y / 2.0),
            size,
            size_color,
        ),
    ));
}

pub fn paint_folder_label(
    painter: &Painter,
    rect: Rect,
    data: &Dataset,
    id: Id,
    font_size: f32,
    name_color: Color32,
    size_color: Color32,
) {
    let mut labels = Vec::with_capacity(2);
    folder_labels(
        &mut labels,
        painter,
        rect,
        data,
        id,
        font_size,
        (name_color, size_color),
    );
    for (clip, shape) in labels {
        painter.with_clip_rect(clip).add(shape);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        layout::{self, Detail, View},
        model::{self, ROOT},
    };
    use std::sync::atomic::AtomicBool;
    #[test]
    fn dense_mesh_is_valid_and_extensions_keep_their_colours() {
        assert_eq!(file_color("movie.mp4"), file_color("MOVIE.MP4"));
        let colours: std::collections::HashSet<_> = [
            "a.vhdx",
            "a.mp4",
            "a.wav",
            "a.png",
            "a.zip",
            "a.rs",
            "a.db",
            "a.pdf",
            "a.dll",
            "a.blend",
            "a.unknown",
        ]
        .map(file_color)
        .into_iter()
        .collect();
        assert_eq!(colours.len(), 11);
        let data = model::synthetic(100_000, false, &AtomicBool::new(false), |_, _, _| {}).unwrap();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1600.0, 900.0));
        let layout = layout::build(
            &data,
            View::folder(&data, ROOT),
            rect,
            Detail::Pixel.settings(64),
        );
        let ctx = egui::Context::default();
        let mut render = Render::default();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ctx| {
                let painter = ctx.layer_painter(egui::LayerId::background());
                render = Render::prepare(&painter, &data, &layout);
            },
        );
        assert!(render.mesh.is_valid());
        assert!(!render.mesh.is_empty());
        assert!(render.files > 0);
        assert!(
            render
                .mesh
                .vertices
                .iter()
                .all(|v| v.pos.is_finite() && rect.contains(v.pos))
        );
        assert!(render.mesh.vertices.len() <= layout.tiles.len() * 9);
    }
}
