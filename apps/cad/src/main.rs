#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    // `fuide-cad --mcp`: stdio MCP bridge to the running app (see `fuide::agent::bridge`)
    if std::env::args().nth(1).as_deref() == Some("--mcp") {
        std::process::exit(fuide::agent::bridge::run_with_tools(
            "cad",
            "FUIDE CAD",
            &fuide_cad::app::tools::tools(),
        ));
    }
    fuide::devshot::install_trace_logger();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("FUIDE CAD")
            .with_app_id("fuide-cad")
            .with_decorations(false)
            .with_transparent(true)
            .with_has_shadow(false)
            .with_inner_size([1380.0, 860.0])
            .with_min_inner_size([1200.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "FUIDE CAD",
        options,
        Box::new(|cc| Ok(Box::new(fuide_cad::app::CadApp::new(cc)))),
    )
}
