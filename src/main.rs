#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod loader;
mod shots;
mod state;

fn main() -> eframe::Result {
    // Un PDF passé en argument (Ronnie, « Ouvrir avec », ligne de commande).
    let open = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Serj")
            .with_inner_size([1100.0, 800.0])
            .with_min_inner_size([480.0, 360.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "Serj",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc, open)))),
    )
}
