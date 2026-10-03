use crate::browser::{Browser, COLUMN_PADDING_X, column_width, viewport_offset};
use eframe::egui;
use std::time::Duration;

const ROW_HEIGHT: f32 = 52.0;
const COLUMN_SLIDE_DURATION: Duration = Duration::from_millis(200);
#[derive(Clone, Copy)]
enum ColumnRole {
    Ancestor,
    Current,
}
#[derive(Clone, Copy)]
enum RowState {
    Normal,
    PathSelected,
    Focused,
}

#[derive(Default)]
pub struct Camera {
    offset: f32,
    target: f32,
    from: f32,
    started: f64,
}
impl Camera {
    fn update(&mut self, target: f32, now: f64) -> bool {
        let t = ((now - self.started) / COLUMN_SLIDE_DURATION.as_secs_f64()).clamp(0.0, 1.0) as f32;
        self.offset = self.from + (self.target - self.from) * (1.0 - (1.0 - t).powi(3));
        if target != self.target {
            self.from = self.offset;
            self.target = target;
            self.started = now;
        }
        self.offset != self.target
    }
}

pub fn show(
    ui: &mut egui::Ui,
    browser: &mut Browser,
    camera: &mut Camera,
    ensure_visible: bool,
) -> Option<usize> {
    let font = egui::FontId::proportional(30.0);
    for column in &mut browser.columns {
        if column.width_pending {
            column.width = column_width(&column.entries, |name| {
                ui.painter()
                    .layout_no_wrap(name.to_owned(), font.clone(), egui::Color32::WHITE)
                    .size()
                    .x
            });
            column.width_pending = false;
        }
    }
    let viewport = ui.available_rect_before_wrap();
    let widths: Vec<_> = browser.columns.iter().map(|c| c.width).collect();
    let target = viewport_offset(&widths, viewport.width());
    if camera.update(target, ui.input(|i| i.time)) {
        ui.ctx().request_repaint();
    }
    let count = browser.columns.len();
    let mut x = viewport.left() + camera.offset;
    let mut clicked = None;
    for (index, column) in browser.columns.iter_mut().enumerate() {
        let role = if index + 1 == count {
            ColumnRole::Current
        } else {
            ColumnRole::Ancestor
        };
        // The current directory has no following hierarchy boundary. Its rows
        // fill the viewport; the retained width still positions future columns.
        let render_width = match role {
            ColumnRole::Current => (viewport.right() - x).max(0.0),
            ColumnRole::Ancestor => column.width,
        };
        let rect = egui::Rect::from_min_size(
            egui::pos2(x, viewport.top()),
            egui::vec2(render_width, viewport.height()),
        );
        let clip = rect.intersect(viewport);
        if matches!(role, ColumnRole::Current) {
            if ensure_visible && !column.entries.is_empty() {
                let top = column.view.selected as f32 * ROW_HEIGHT;
                if top < column.view.scroll {
                    column.view.scroll = top;
                }
                if top + ROW_HEIGHT > column.view.scroll + viewport.height() {
                    column.view.scroll = top + ROW_HEIGHT - viewport.height();
                }
            }
            if ui.rect_contains_pointer(clip) {
                column.view.scroll -= ui.input(|i| i.smooth_scroll_delta.y);
            }
        }
        column.view.scroll = column.view.scroll.clamp(
            0.0,
            (column.entries.len() as f32 * ROW_HEIGHT - viewport.height()).max(0.0),
        );
        let painter = ui.painter().with_clip_rect(clip);
        if index > 0 {
            painter.line_segment(
                [rect.left_top(), rect.left_bottom()],
                egui::Stroke::new(1.0_f32, egui::Color32::from_gray(70)),
            );
        }
        for (row, entry) in column.entries.iter().enumerate() {
            let row_rect = egui::Rect::from_min_size(
                egui::pos2(
                    x,
                    viewport.top() + row as f32 * ROW_HEIGHT - column.view.scroll,
                ),
                egui::vec2(render_width, ROW_HEIGHT),
            );
            if !row_rect.intersects(clip) {
                continue;
            }
            let state = if row == column.view.selected {
                match role {
                    ColumnRole::Current => RowState::Focused,
                    ColumnRole::Ancestor => RowState::PathSelected,
                }
            } else {
                RowState::Normal
            };
            let foreground = match role {
                ColumnRole::Current => egui::Color32::WHITE,
                ColumnRole::Ancestor => egui::Color32::from_gray(135),
            };
            match state {
                RowState::Focused => {
                    painter.rect_filled(row_rect, 0.0, egui::Color32::from_gray(65));
                }
                RowState::PathSelected => {
                    painter.rect_filled(row_rect, 0.0, egui::Color32::from_gray(35));
                }
                RowState::Normal => {}
            }
            let galley = painter.layout_no_wrap(
                entry.name.to_string_lossy().into_owned(),
                font.clone(),
                foreground,
            );
            let overflow = galley.size().x > render_width - 2.0 * COLUMN_PADDING_X;
            let text_clip = row_rect
                .shrink2(egui::vec2(COLUMN_PADDING_X, 0.0))
                .intersect(clip);
            painter.with_clip_rect(text_clip).galley(
                egui::pos2(
                    x + COLUMN_PADDING_X,
                    row_rect.center().y - galley.size().y / 2.0,
                ),
                galley,
                foreground,
            );
            if overflow {
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(
                            rect.right() - COLUMN_PADDING_X - 12.0,
                            row_rect.center().y - 12.0,
                        ),
                        egui::vec2(12.0, 24.0),
                    ),
                    0.0,
                    foreground,
                );
            }
            if matches!(role, ColumnRole::Current)
                && ui
                    .interact(
                        row_rect.intersect(clip),
                        ui.id().with((index, row)),
                        egui::Sense::click(),
                    )
                    .clicked_by(egui::PointerButton::Primary)
            {
                clicked = Some(row);
            }
        }
        x += column.width;
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn camera_eases_and_reverses_from_its_present_position() {
        let mut camera = Camera::default();
        assert!(camera.update(-300.0, 1.0));
        assert_eq!(camera.offset, 0.0);
        assert!(camera.update(-300.0, 1.1));
        assert!((camera.offset + 262.5).abs() < 0.01);
        let position = camera.offset;
        assert!(camera.update(0.0, 1.1));
        assert_eq!(camera.offset, position);
        assert!(!camera.update(0.0, 1.4));
        assert_eq!(camera.offset, 0.0);
    }
    #[test]
    fn rendering_measures_once_and_keeps_selection_geometry_stable() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(
            temp.path()
                .join("An unusually long directory name for measurement"),
        )
        .unwrap();
        std::fs::write(temp.path().join("video.mkv"), b"").unwrap();
        let mut browser = Browser::new(Some(temp.path().to_owned()));
        let context = egui::Context::default();
        let mut camera = Camera::default();
        let mut render = |browser: &mut Browser| {
            let _ = context.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show(ui, browser, &mut camera, true);
                });
            });
        };
        render(&mut browser);
        let width = browser.columns[0].width;
        assert!(width > crate::browser::MIN_COLUMN_WIDTH);
        assert!(!browser.columns[0].width_pending);
        browser.select(1);
        render(&mut browser);
        assert_eq!(browser.columns[0].width, width);
    }
}
