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
