mod alert;
mod app;
mod axis;
mod calc;
mod candle;
mod compare;
mod data;
mod indicator;
mod mail;
mod store;
mod theme;

fn main() -> eframe::Result {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("StockView")
            .with_inner_size([1480.0, 900.0])
            .with_min_inner_size([1000.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native("StockView", opts, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
