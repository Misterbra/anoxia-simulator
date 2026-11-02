//! Anoxia Simulator — desktop application (egui/eframe).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![warn(clippy::all)]

mod analysis_tab;
mod app;
mod examples;
mod help;
mod inputs;
mod plots;
mod report_tab;
mod results;
mod settings;
mod theme;
mod tutorial;

use eframe::egui;

fn main() -> eframe::Result<()> {
    std::panic::set_hook(Box::new(|info| {
        let path = std::env::temp_dir().join("anoxia-simulator-crash.log");
        let _ = std::fs::write(&path, format!("Anoxia Simulator v{}\n{info}\n", anoxia_core::VERSION));
    }));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("Anoxia Simulator {}", anoxia_core::VERSION))
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1024.0, 640.0])
            .with_icon(theme::app_icon()),
        ..Default::default()
    };
    let outcome = eframe::run_native(
        "Anoxia Simulator",
        options,
        Box::new(|cc| {
            theme::setup(&cc.egui_ctx);
            let app = app::AnoxiaApp::new();
            theme::apply_visuals(&cc.egui_ctx, app.dark);
            Ok(Box::new(app))
        }),
    );
    if let Err(e) = &outcome {
        // No console in release builds: tell the user why the window did not open.
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Anoxia Simulator")
            .set_description(format!(
                "Impossible de démarrer l'interface graphique / cannot start the graphical interface:\n{e}\n\nVérifiez les pilotes graphiques (OpenGL) / check the graphics drivers."
            ))
            .show();
    }
    outcome
}
