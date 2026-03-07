use std::collections::HashSet;

use eframe::egui;

use crate::model::{BOARD_SIZE, Cell, Pos};
use crate::usecase::{GameController, GameViewModel};

const BOARD_PIXEL_SIZE: f32 = 520.0;
const CELL_PADDING: f32 = 3.0;
const ANIM_SECONDS: f64 = 0.24;
const LABEL_GUTTER: f32 = 22.0;

pub struct OthelloApp {
    controller: GameController,
    last_anim: Option<(Pos, f64)>,
}

impl Default for OthelloApp {
    fn default() -> Self {
        Self {
            controller: GameController::new(),
            last_anim: None,
        }
    }
}

impl eframe::App for OthelloApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut vm = self.controller.view_model();

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Othello");
                ui.separator();
                ui.label(vm.message.as_str());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Reset").clicked() {
                        self.controller.reset();
                        self.last_anim = None;
                        vm = self.controller.view_model();
                    }
                });
            });
        });

        egui::SidePanel::right("side_panel")
            .min_width(220.0)
            .show(ctx, |ui| {
                draw_turn_badge(ui, vm.turn);
                ui.add_space(8.0);
                draw_score_bar(ui, vm.black_score, vm.white_score);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(vm.can_undo, egui::Button::new("Undo"))
                        .clicked()
                    {
                        self.controller.undo();
                        vm = self.controller.view_model();
                    }
                    if ui
                        .add_enabled(vm.can_redo, egui::Button::new("Redo"))
                        .clicked()
                    {
                        self.controller.redo();
                        vm = self.controller.view_model();
                    }
                });
                ui.add_space(10.0);
                ui.heading("History");
                egui::ScrollArea::vertical()
                    .id_salt("history_scroll")
                    .max_height(ui.available_height())
                    .show(ui, |ui| {
                        if vm.history_lines.is_empty() {
                            ui.label("No moves yet.");
                        } else {
                            for line in &vm.history_lines {
                                ui.monospace(line);
                            }
                        }
                    });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                let clicked = draw_board(ui, ctx, &vm, self.last_anim);
                if let Some(pos) = clicked {
                    let before = vm.last_move;
                    self.controller.click_cell(pos);
                    vm = self.controller.view_model();
                    if vm.last_move != before
                        && let Some(last_move) = vm.last_move
                    {
                        self.last_anim = Some((last_move, ctx.input(|i| i.time)));
                    }
                }
            });
        });

        if vm.game_over {
            egui::Window::new("Result")
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.heading("Game Over");
                    ui.label(format!(
                        "Black: {} / White: {}",
                        vm.black_score, vm.white_score
                    ));
                    ui.label(vm.message.as_str());
                    ui.add_space(8.0);
                    if ui.button("Play Again").clicked() {
                        self.controller.reset();
                        self.last_anim = None;
                    }
                });
        }

        if let Some((_, start)) = self.last_anim {
            let now = ctx.input(|i| i.time);
            if now - start < ANIM_SECONDS {
                ctx.request_repaint();
            } else {
                self.last_anim = None;
            }
        }
    }
}

fn draw_turn_badge(ui: &mut egui::Ui, turn: Cell) {
    let (text, color) = match turn {
        Cell::Black => ("Turn: Black", egui::Color32::from_rgb(20, 20, 20)),
        Cell::White => ("Turn: White", egui::Color32::from_rgb(230, 230, 230)),
        Cell::Empty => ("Turn: -", egui::Color32::GRAY),
    };
    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgb(238, 246, 238))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(color, "●");
                ui.label(text);
            });
        });
}

fn draw_score_bar(ui: &mut egui::Ui, black: usize, white: usize) {
    let total = (black + white).max(1) as f32;
    let black_ratio = black as f32 / total;
    let desired = egui::vec2(ui.available_width(), 22.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());

    let painter = ui.painter();
    let split = rect.left() + rect.width() * black_ratio;
    let black_rect = egui::Rect::from_min_max(rect.min, egui::pos2(split, rect.max.y));
    let white_rect = egui::Rect::from_min_max(egui::pos2(split, rect.min.y), rect.max);

    painter.rect_filled(black_rect, 3.0, egui::Color32::from_rgb(24, 24, 24));
    painter.rect_filled(white_rect, 3.0, egui::Color32::from_rgb(235, 235, 235));
    painter.rect_stroke(
        rect,
        3.0,
        egui::Stroke::new(1.0, egui::Color32::from_gray(110)),
        egui::StrokeKind::Outside,
    );

    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.label(format!("Black: {black}"));
        ui.separator();
        ui.label(format!("White: {white}"));
    });
}

fn draw_board(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    vm: &GameViewModel,
    anim: Option<(Pos, f64)>,
) -> Option<Pos> {
    let board_size = BOARD_PIXEL_SIZE
        .min(ui.available_width())
        .min(ui.available_height());
    let total_size = board_size + LABEL_GUTTER * 2.0;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(total_size, total_size), egui::Sense::click());
    let painter = ui.painter_at(rect);

    let board_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + LABEL_GUTTER, rect.top() + LABEL_GUTTER),
        egui::pos2(rect.right() - LABEL_GUTTER, rect.bottom() - LABEL_GUTTER),
    );
    let cell_size = board_size / BOARD_SIZE as f32;

    painter.rect_filled(rect, 6.0, egui::Color32::from_rgb(20, 20, 20));
    let board_color = egui::Color32::from_rgb(30, 124, 73);
    painter.rect_filled(board_rect, 6.0, board_color);

    let legal: HashSet<Pos> = vm.legal_moves.iter().copied().collect();
    let now = ctx.input(|i| i.time);

    for y in 0..BOARD_SIZE {
        for x in 0..BOARD_SIZE {
            let min = egui::pos2(
                board_rect.left() + x as f32 * cell_size,
                board_rect.top() + y as f32 * cell_size,
            );
            let max = egui::pos2(min.x + cell_size, min.y + cell_size);
            let cell_rect = egui::Rect::from_min_max(min, max);
            painter.rect_stroke(
                cell_rect,
                0.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(8, 64, 36)),
                egui::StrokeKind::Inside,
            );

            let pos = Pos::new(x, y);
            if vm.last_move == Some(pos) {
                painter.rect_filled(
                    cell_rect.shrink(1.0),
                    0.0,
                    egui::Color32::from_rgba_premultiplied(250, 214, 79, 72),
                );
            }

            let center = cell_rect.center();
            let stone_radius = (cell_size * 0.5) - CELL_PADDING;
            match vm.board[y][x] {
                Cell::Black | Cell::White => {
                    let mut r = stone_radius;
                    if let Some((animated_pos, start)) = anim
                        && animated_pos == pos
                    {
                        let t = ((now - start) / ANIM_SECONDS).clamp(0.0, 1.0) as f32;
                        r *= 0.65 + 0.35 * t;
                    }
                    let color = match vm.board[y][x] {
                        Cell::Black => egui::Color32::from_rgb(15, 15, 15),
                        Cell::White => egui::Color32::from_rgb(246, 246, 246),
                        Cell::Empty => unreachable!(),
                    };
                    painter.circle_filled(center, r, color);
                    painter.circle_stroke(
                        center,
                        r,
                        egui::Stroke::new(1.0, egui::Color32::from_gray(50)),
                    );
                }
                Cell::Empty => {
                    if legal.contains(&pos) {
                        let preview = match vm.turn {
                            Cell::Black => egui::Color32::from_rgba_premultiplied(20, 20, 20, 200),
                            Cell::White => {
                                egui::Color32::from_rgba_premultiplied(250, 250, 250, 210)
                            }
                            Cell::Empty => {
                                egui::Color32::from_rgba_premultiplied(255, 255, 255, 170)
                            }
                        };
                        painter.circle_filled(center, cell_size * 0.08, preview);
                    }
                }
            }
        }
    }

    let label_color = egui::Color32::from_rgb(245, 245, 245);
    let font_id = egui::FontId::proportional(16.0);
    for x in 0..BOARD_SIZE {
        let ch = (b'a' + x as u8) as char;
        let x_center = board_rect.left() + x as f32 * cell_size + cell_size * 0.5;
        painter.text(
            egui::pos2(x_center, rect.top() + LABEL_GUTTER * 0.5),
            egui::Align2::CENTER_CENTER,
            ch,
            font_id.clone(),
            label_color,
        );
    }
    for y in 0..BOARD_SIZE {
        let rank = (y + 1).to_string();
        let y_center = board_rect.top() + y as f32 * cell_size + cell_size * 0.5;
        painter.text(
            egui::pos2(rect.left() + LABEL_GUTTER * 0.5, y_center),
            egui::Align2::CENTER_CENTER,
            rank.as_str(),
            font_id.clone(),
            label_color,
        );
    }

    if response.clicked()
        && !vm.game_over
        && let Some(pointer_pos) = response.interact_pointer_pos()
    {
        let rel_x = ((pointer_pos.x - board_rect.left()) / cell_size).floor() as isize;
        let rel_y = ((pointer_pos.y - board_rect.top()) / cell_size).floor() as isize;
        if (0..BOARD_SIZE as isize).contains(&rel_x) && (0..BOARD_SIZE as isize).contains(&rel_y) {
            return Some(Pos::new(rel_x as usize, rel_y as usize));
        }
    }
    None
}
