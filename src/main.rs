use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();

    eframe::run_simple_native("Hello egui", options, |ctx, _frame| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hello world");
        });
    })
}
