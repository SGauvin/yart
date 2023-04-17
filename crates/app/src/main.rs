#![warn(clippy::all, rust_2018_idioms)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    re_log::setup_native_logging();

    let native_options = eframe::NativeOptions {
        initial_window_size: Some([1200.0, 800.0].into()),
        follow_system_theme: false,
        default_theme: eframe::Theme::Dark,
        ..Default::default()
    };

    eframe::run_native(
        "App",
        native_options,
        Box::new(move |cc| Box::new(app::ExampleApp::new(cc))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    re_log::setup_web_logging();
    // Make sure panics are logged using `console.error`.
    console_error_panic_hook::set_once();

    // Redirect tracing to console.log and friends:
    tracing_wasm::set_as_global_default();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        eframe::start_web(
            "the_canvas_id", // hardcode it
            web_options,
            Box::new(move |cc| Box::new(app::ExampleApp::new(cc))),
        )
        .await
        .expect("failed to start eframe");
    });
}
