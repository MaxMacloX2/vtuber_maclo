#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // Oculta la consola negra en Windows

//! Punto de entrada de `vtuber_maclo`: carga `vtuber_maclo.toml`, configura la ventana nativa
//! transparente y arranca el ciclo principal de `eframe`.

mod animation;
mod app;
mod audio;
mod avatar;
mod config;
mod input;
mod ui;

use crate::app::VtuberApp;
use eframe::egui;

/// Carga la configuración externa, configura `eframe` e instala los cargadores de imagen PNG.
fn main() -> eframe::Result<()> {
    let config_path = config::resolve_default_config_path();
    let loaded = config::load_or_create_config(&config_path);

    let window_cfg = loaded.app_config.window.clone();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(window_cfg.inner_size)
            .with_transparent(window_cfg.transparent)
            .with_decorations(window_cfg.decorations),
        ..Default::default()
    };

    eframe::run_native(
        &window_cfg.title,
        options,
        Box::new(move |cc| {
            // Registrar cargadores de imágenes PNG para sprites incrustados y precargados en bytes.
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(VtuberApp::new(loaded)))
        }),
    )
}
