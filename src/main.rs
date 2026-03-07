mod model;
mod ui;
mod usecase;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Othello",
        options,
        Box::new(|_cc| Ok(Box::new(ui::OthelloApp::default()))),
    )
}
