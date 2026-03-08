use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver};

use eframe::egui;

use crate::cpu::{CpuMoveResult, Difficulty, OpenAiClient, TokenUsage};
use crate::model::{BOARD_SIZE, Cell, Pos};
use crate::usecase::{GameController, GameViewModel};

const BOARD_PIXEL_SIZE: f32 = 680.0;
const CELL_PADDING: f32 = 3.0;
const ANIM_SECONDS: f64 = 0.24;
const LABEL_GUTTER: f32 = 22.0;
const DEFAULT_MODEL: &str = "gpt-5-mini";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppScreen {
    Start,
    Game,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlayerKind {
    Human,
    Cpu,
}

impl PlayerKind {
    fn name(self) -> &'static str {
        match self {
            Self::Human => "Human",
            Self::Cpu => "CPU",
        }
    }
}

#[derive(Clone, Debug)]
struct PlayerConfig {
    kind: PlayerKind,
    difficulty: Difficulty,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            kind: PlayerKind::Human,
            difficulty: Difficulty::Normal,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct StartConfig {
    api_key: String,
    black: PlayerConfig,
    white: PlayerConfig,
}

#[derive(Clone, Debug, Default)]
struct UsageStats {
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    cumulative_cost_usd: f64,
    last_call_tokens: u64,
    last_call_cost_usd: f64,
}

impl UsageStats {
    fn add_call(&mut self, usage: TokenUsage) {
        self.input_tokens += usage.input_tokens;
        self.output_tokens += usage.output_tokens;
        self.total_tokens += usage.total_tokens;
        self.cumulative_cost_usd += usage.estimated_cost_usd;
        self.last_call_tokens = usage.total_tokens;
        self.last_call_cost_usd = usage.estimated_cost_usd;
    }
}

pub struct OthelloApp {
    screen: AppScreen,
    start_config: StartConfig,
    controller: GameController,
    last_anim: Option<(Pos, f64)>,
    usage: UsageStats,
    cpu_thinking: bool,
    cpu_status: Option<String>,
    cpu_rx: Option<Receiver<CpuMoveResult>>,
}

impl Default for OthelloApp {
    fn default() -> Self {
        Self {
            screen: AppScreen::Start,
            start_config: StartConfig::default(),
            controller: GameController::new(),
            last_anim: None,
            usage: UsageStats::default(),
            cpu_thinking: false,
            cpu_status: None,
            cpu_rx: None,
        }
    }
}

impl eframe::App for OthelloApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        match self.screen {
            AppScreen::Start => self.update_start_screen(ctx),
            AppScreen::Game => self.update_game_screen(ctx),
        }
    }
}

impl OthelloApp {
    fn update_start_screen(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.heading("Othello - Start");
                ui.label("Configure each side independently");
                ui.add_space(12.0);
            });

            ui.columns(2, |cols| {
                draw_player_config(
                    &mut cols[0],
                    "Player 1 (Black)",
                    &mut self.start_config.black,
                );
                draw_player_config(
                    &mut cols[1],
                    "Player 2 (White)",
                    &mut self.start_config.white,
                );
            });

            ui.add_space(12.0);
            let cpu_exists = self.has_any_cpu();
            ui.group(|ui| {
                ui.label("OpenAI API Key (memory only)");
                ui.add(
                    egui::TextEdit::singleline(&mut self.start_config.api_key)
                        .password(true)
                        .hint_text("sk-...")
                        .desired_width(560.0),
                );
                if cpu_exists {
                    ui.small("Required when either side is CPU.");
                } else {
                    ui.small("Optional when both sides are Human.");
                }
            });

            let can_start = !cpu_exists || !self.start_config.api_key.trim().is_empty();
            if cpu_exists && !can_start {
                ui.colored_label(
                    egui::Color32::from_rgb(190, 60, 60),
                    "At least one side is CPU, so API key is required.",
                );
            }

            ui.add_space(12.0);
            if ui
                .add_enabled(can_start, egui::Button::new("Start Game"))
                .clicked()
            {
                self.controller.reset();
                self.last_anim = None;
                self.usage = UsageStats::default();
                self.cpu_status = None;
                self.cpu_thinking = false;
                self.cpu_rx = None;
                self.screen = AppScreen::Game;
            }
        });
    }

    fn update_game_screen(&mut self, ctx: &egui::Context) {
        self.poll_cpu_result(ctx);
        let mut vm = self.controller.view_model();

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Othello");
                ui.separator();
                ui.label(vm.message.as_str());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Back to Start").clicked() {
                        self.screen = AppScreen::Start;
                    }
                    if ui.button("Reset").clicked() {
                        self.controller.reset();
                        self.last_anim = None;
                        self.cpu_thinking = false;
                        self.cpu_status = None;
                        self.cpu_rx = None;
                        self.usage = UsageStats::default();
                        vm = self.controller.view_model();
                    }
                });
            });
        });

        egui::SidePanel::right("side_panel")
            .min_width(320.0)
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

                ui.add_space(12.0);
                ui.heading("Players");
                draw_player_summary(ui, Cell::Black, &self.start_config.black);
                draw_player_summary(ui, Cell::White, &self.start_config.white);

                ui.add_space(8.0);
                ui.heading("CPU Status");
                if self.cpu_thinking {
                    ui.colored_label(egui::Color32::from_rgb(40, 120, 200), "Thinking...");
                } else {
                    ui.label("Idle");
                }
                if let Some(status) = &self.cpu_status {
                    ui.small(status);
                }

                ui.add_space(12.0);
                ui.heading("Token / Cost");
                ui.monospace(format!("Last call tokens: {}", self.usage.last_call_tokens));
                ui.monospace(format!(
                    "Last call cost: ${:.6}",
                    self.usage.last_call_cost_usd
                ));
                ui.separator();
                ui.monospace(format!("Input tokens: {}", self.usage.input_tokens));
                ui.monospace(format!("Output tokens: {}", self.usage.output_tokens));
                ui.monospace(format!("Total tokens: {}", self.usage.total_tokens));
                ui.monospace(format!(
                    "Total cost: ${:.6}",
                    self.usage.cumulative_cost_usd
                ));

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
                let clickable = !vm.game_over && !self.cpu_thinking && self.is_human_turn(vm.turn);
                let clicked = draw_board(ui, ctx, &vm, self.last_anim, clickable);
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
                        self.cpu_status = None;
                        self.cpu_thinking = false;
                        self.cpu_rx = None;
                        self.usage = UsageStats::default();
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

        self.maybe_schedule_cpu(&vm);
        if self.cpu_thinking {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    fn has_any_cpu(&self) -> bool {
        self.start_config.black.kind == PlayerKind::Cpu
            || self.start_config.white.kind == PlayerKind::Cpu
    }

    fn player_config_for(&self, turn: Cell) -> &PlayerConfig {
        match turn {
            Cell::Black => &self.start_config.black,
            Cell::White => &self.start_config.white,
            Cell::Empty => &self.start_config.black,
        }
    }

    fn is_human_turn(&self, turn: Cell) -> bool {
        self.player_config_for(turn).kind == PlayerKind::Human
    }

    fn maybe_schedule_cpu(&mut self, vm: &GameViewModel) {
        if vm.game_over
            || self.cpu_thinking
            || vm.legal_moves.is_empty()
            || self.is_human_turn(vm.turn)
        {
            return;
        }

        self.cpu_thinking = true;
        self.cpu_status = Some(format!(
            "{} CPU: calling OpenAI Responses API...",
            vm.turn.name()
        ));

        let board = vm.board;
        let turn = vm.turn;
        let legal_moves = vm.legal_moves.clone();
        let difficulty = self.player_config_for(turn).difficulty;
        let api_key = self.start_config.api_key.clone();
        let (tx, rx) = mpsc::channel();
        self.cpu_rx = Some(rx);

        std::thread::spawn(move || {
            let result = match OpenAiClient::new(api_key, DEFAULT_MODEL.to_string()) {
                Ok(client) => client.choose_move(&board, turn, &legal_moves, difficulty),
                Err(err) => CpuMoveResult {
                    pos: *legal_moves.first().unwrap_or(&Pos::new(0, 0)),
                    usage: None,
                    fallback_used: true,
                    note: Some(format!("CPU fallback move used: {err}")),
                },
            };
            let _ = tx.send(result);
        });
    }

    fn poll_cpu_result(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.cpu_rx else {
            return;
        };
        let Ok(result) = rx.try_recv() else {
            return;
        };

        self.cpu_rx = None;
        self.cpu_thinking = false;

        if let Some(usage) = result.usage {
            self.usage.add_call(usage);
        }

        let turn = self.controller.view_model().turn;
        let note = if result.fallback_used {
            result.note.clone()
        } else {
            None
        };
        if let Some(text) = &result.note {
            self.cpu_status = Some(format!("{} CPU: {text}", turn.name()));
        } else {
            self.cpu_status = Some(format!(
                "{} CPU played {}",
                turn.name(),
                result.pos.notation()
            ));
        }

        let before = self.controller.view_model().last_move;
        self.controller.apply_move_with_note(result.pos, note);
        let after = self.controller.view_model();
        if after.last_move != before
            && let Some(last_move) = after.last_move
        {
            self.last_anim = Some((last_move, ctx.input(|i| i.time)));
        }
    }
}

fn draw_player_config(ui: &mut egui::Ui, title: &str, config: &mut PlayerConfig) {
    ui.group(|ui| {
        ui.heading(title);
        ui.add_space(6.0);
        ui.label("Type");
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut config.kind,
                PlayerKind::Human,
                PlayerKind::Human.name(),
            );
            ui.selectable_value(&mut config.kind, PlayerKind::Cpu, PlayerKind::Cpu.name());
        });

        ui.add_space(8.0);
        ui.add_enabled_ui(config.kind == PlayerKind::Cpu, |ui| {
            ui.label("CPU Difficulty");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut config.difficulty, Difficulty::Easy, "Easy");
                ui.selectable_value(&mut config.difficulty, Difficulty::Normal, "Normal");
                ui.selectable_value(&mut config.difficulty, Difficulty::Hard, "Hard");
            });
        });
    });
}

fn draw_player_summary(ui: &mut egui::Ui, side: Cell, config: &PlayerConfig) {
    let side_name = match side {
        Cell::Black => "Black",
        Cell::White => "White",
        Cell::Empty => "-",
    };
    if config.kind == PlayerKind::Cpu {
        ui.label(format!("{side_name}: CPU ({})", config.difficulty.name()));
    } else {
        ui.label(format!("{side_name}: Human"));
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
    clickable: bool,
) -> Option<Pos> {
    let board_size = BOARD_PIXEL_SIZE
        .min(ui.available_width())
        .min(ui.available_height());
    let total_size = board_size + LABEL_GUTTER * 2.0;
    let sense = if clickable {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(total_size, total_size), sense);
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

    if clickable
        && response.clicked()
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
