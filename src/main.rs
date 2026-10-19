#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod infos;
mod library;
mod loader;
mod shots;
mod state;
mod widgets;

/// Linux : relie la fenêtre à son icône (Wayland et X11).
const APP_ID: &str = "serj";

fn main() -> eframe::Result {
    let arg = std::env::args_os().nth(1);
    if arg.as_deref() == Some(std::ffi::OsStr::new("--version")) {
        println!("serj {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    // Débogage : les infos trouvées pour des albums, sans ouvrir la fenêtre.
    if arg.as_deref() == Some(std::ffi::OsStr::new("--infos")) {
        let manga = std::env::var_os("SERJ_MANGA").is_some();
        for path in std::env::args_os().skip(2) {
            let info = infos::lookup_now(std::path::Path::new(&path), manga);
            println!(
                "{}",
                serde_json::to_string_pretty(&info).unwrap_or_default()
            );
        }
        return Ok(());
    }
    // Un PDF passé en argument (Ronnie, « Ouvrir avec », ligne de commande).
    let open = arg.map(std::path::PathBuf::from);

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Serj")
        .with_app_id(APP_ID)
        .with_inner_size([1100.0, 800.0])
        // Ouverte en grand : toute la place de l'écran, mais redimensionnable.
        .with_maximized(true)
        .with_min_inner_size([480.0, 360.0])
        .with_drag_and_drop(true);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon/icon.png")) {
        viewport = viewport.with_icon(icon);
    }
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    // Windows : DirectX 12 plutôt que Vulkan, que certains pilotes Intel font
    // planter dès le chargement (comme pour Ronnie). WGPU_BACKEND choisit encore.
    #[cfg(windows)]
    if std::env::var_os("WGPU_BACKEND").is_none() {
        let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
        setup.instance_descriptor.backends =
            eframe::wgpu::Backends::DX12 | eframe::wgpu::Backends::GL;
        options.wgpu_options.wgpu_setup = setup.into();
    }
    eframe::run_native(
        "Serj",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc, open)))),
    )
}
