use eframe::egui::{self, Color32, Pos2, Response, Sense, Stroke, Ui, vec2};

const INFO: &str = "fnoob_info_text";

pub fn hint(response: Response, text: &str) -> Response {
    if response.hovered() {
        response
            .ctx
            .data_mut(|d| d.insert_temp(egui::Id::unique(INFO), text.to_owned()));
    }
    response
}

pub fn take_info(ctx: &egui::Context) -> Option<String> {
    ctx.data_mut(|d| d.remove_temp::<String>(egui::Id::unique(INFO)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Stop,
    Record,
}

pub fn icon_button(ui: &mut Ui, icon: Icon, on: bool, on_color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(28.0, 22.0), Sense::click());
    let visuals = ui.style().interact_selectable(&response, on);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, visuals.weak_bg_fill);
    let color = if on {
        on_color
    } else {
        visuals.fg_stroke.color
    };
    let c = rect.center();
    match icon {
        Icon::Play => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(c.x - 5.0, c.y - 6.0),
                    Pos2::new(c.x + 6.0, c.y),
                    Pos2::new(c.x - 5.0, c.y + 6.0),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Stop => {
            painter.rect_filled(
                egui::Rect::from_center_size(c, vec2(10.0, 10.0)),
                1.0,
                color,
            );
        }
        Icon::Record => {
            painter.circle_filled(c, 6.0, color);
        }
    }
    response
}

pub fn compact(ui: &mut Ui) {
    let style = ui.style_mut();
    style.override_text_style = Some(egui::TextStyle::Small);
    style.spacing.button_padding = vec2(4.0, 0.0);
    style.spacing.interact_size.y = 16.0;
    style.spacing.item_spacing = vec2(2.0, 2.0);
}

pub fn knob(ui: &mut Ui, value: &mut f32, min: f32, max: f32, label: &str) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(26.0, 34.0), Sense::click_and_drag());
    if response.dragged() {
        let delta = -response.drag_delta().y;
        let next = (*value + delta * (max - min) / 150.0).clamp(min, max);
        if next != *value {
            *value = next;
            response.mark_changed();
        }
    }
    if response.double_clicked() {
        *value = 0.0f32.clamp(min, max);
        response.mark_changed();
    }
    let painter = ui.painter_at(rect);
    let center = Pos2::new(rect.center().x, rect.top() + 12.0);
    let radius = 9.0;
    let visuals = ui.style().interact(&response);
    painter.circle_stroke(center, radius, Stroke::new(1.5, visuals.fg_stroke.color));
    let fraction = if max > min {
        (*value - min) / (max - min)
    } else {
        0.0
    };
    let angle = (-135.0 + 270.0 * fraction).to_radians();
    let tip = center + vec2(angle.sin(), -angle.cos()) * radius;
    painter.line_segment(
        [center, tip],
        Stroke::new(2.0, Color32::from_rgb(235, 170, 60)),
    );
    painter.text(
        Pos2::new(rect.center().x, rect.bottom() - 1.0),
        egui::Align2::CENTER_BOTTOM,
        label,
        egui::FontId::proportional(10.0),
        visuals.text_color(),
    );
    response
}

pub fn rename_field(ui: &mut egui::Ui, rect: egui::Rect, text: &mut String) -> Option<bool> {
    let edit = ui.put(rect, egui::TextEdit::singleline(text));
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        return Some(false);
    }
    if edit.lost_focus() {
        return Some(true);
    }
    if !edit.has_focus() {
        edit.request_focus();
        let mut state = egui::TextEdit::load_state(ui.ctx(), edit.id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(text.chars().count()),
            )));
        state.store(ui.ctx(), edit.id);
    }
    None
}
