use eframe::egui::{Color32, Mesh, Painter, Rect, Shape, Stroke, pos2};
use fnoob_engine::audio_data::AudioData;

#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub peak: Color32,
    pub rms: Color32,
    pub axis: Option<Color32>,
    pub split: bool,
}

impl Style {
    pub fn on_clip() -> Self {
        Self {
            peak: Color32::from_black_alpha(120),
            rms: Color32::from_black_alpha(190),
            axis: Some(Color32::from_black_alpha(60)),
            split: true,
        }
    }

    pub fn on_dark(color: Color32) -> Self {
        Self {
            peak: color.gamma_multiply(0.55),
            rms: color,
            axis: Some(Color32::from_white_alpha(28)),
            split: true,
        }
    }
}

const LANE_SPLIT_HEIGHT: f32 = 44.0;

fn strip(points: &[(f32, f32, f32)], color: Color32) -> Shape {
    let mut mesh = Mesh::default();
    for (index, &(x, top, bottom)) in points.iter().enumerate() {
        mesh.colored_vertex(pos2(x, top), color);
        mesh.colored_vertex(pos2(x, bottom.max(top + 0.6)), color);
        if index > 0 {
            let at = (index * 2) as u32;
            mesh.add_triangle(at - 2, at - 1, at);
            mesh.add_triangle(at - 1, at + 1, at);
        }
    }
    Shape::mesh(mesh)
}

pub fn paint(
    painter: &Painter,
    rect: Rect,
    clip: Rect,
    data: &AudioData,
    gain: f32,
    style: Style,
    frame_at: impl Fn(f32) -> f64,
) {
    let visible = rect.intersect(clip);
    if visible.width() <= 0.0 || visible.height() <= 2.0 {
        return;
    }
    let stereo = data.channels > 1;
    let lanes: Vec<(usize, Rect)> = if stereo && style.split && rect.height() >= LANE_SPLIT_HEIGHT {
        let middle = rect.center().y;
        vec![
            (
                0,
                Rect::from_x_y_ranges(rect.x_range(), rect.top()..=middle - 1.0),
            ),
            (
                1,
                Rect::from_x_y_ranges(rect.x_range(), middle + 1.0..=rect.bottom()),
            ),
        ]
    } else {
        vec![(usize::MAX, rect)]
    };
    let left = visible.left().floor();
    let right = visible.right().ceil();
    for (lane, area) in lanes {
        let center = area.center().y;
        let half = area.height() * 0.5;
        if let Some(axis) = style.axis {
            painter.hline(visible.x_range(), center, Stroke::new(1.0, axis));
        }
        let mut peaks = Vec::new();
        let mut rms = Vec::new();
        let mut x = left;
        while x <= right {
            let (from, to) = (frame_at(x), frame_at(x + 1.0));
            let (from, to) = if to > from {
                (from, to)
            } else {
                (from, from + 1.0)
            };
            let summary = if lane == usize::MAX {
                let (a, b) = (data.summary(0, from, to), data.summary(1, from, to));
                fnoob_engine::audio_data::Summary {
                    min: a.min.min(b.min),
                    max: a.max.max(b.max),
                    rms: a.rms.max(b.rms),
                }
            } else {
                data.summary(lane, from, to)
            };
            let high = (summary.max * gain).clamp(-1.0, 1.0);
            let low = (summary.min * gain).clamp(-1.0, 1.0);
            let level = (summary.rms * gain).min(1.0).min(high.max(-low));
            peaks.push((x, center - high * half, center - low * half));
            rms.push((x, center - level * half, center + level * half));
            x += 1.0;
        }
        painter.add(strip(&peaks, style.peak));
        painter.add(strip(&rms, style.rms));
    }
}

pub fn paint_simple(painter: &Painter, rect: Rect, data: &AudioData, color: Color32) {
    let frames = data.frames.max(1) as f64;
    let width = f64::from(rect.width().max(1.0));
    let left = rect.left();
    paint(painter, rect, rect, data, 1.0, Style::on_dark(color), |x| {
        (f64::from(x - left) / width) * frames
    });
}
