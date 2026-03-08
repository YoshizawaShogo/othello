mod cpu;
mod model;
mod ui;
mod usecase;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1920.0, 1080.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Othello",
        options,
        Box::new(|_cc| Ok(Box::new(ui::OthelloApp::default()))),
    )
}
