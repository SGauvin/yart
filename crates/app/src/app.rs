use crate::renderer::Custom3d;

pub struct ExampleApp {
    custom: Custom3d,
}

impl ExampleApp {
    pub fn new<'a>(cc: &'a eframe::CreationContext<'a>) -> Self {
        Self {
            custom: Custom3d::new(cc).expect("Failed to vreate custom 3D renderer"),
        }
    }
}

impl eframe::App for ExampleApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, egui_ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui_ctx.request_repaint();
        egui::gui_zoom::zoom_with_keyboard_shortcuts(
            egui_ctx,
            frame.info().native_pixels_per_point,
        );

        self.top_bar(egui_ctx, frame);

        egui::TopBottomPanel::bottom("bottom_panel").show(egui_ctx, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                egui::warn_if_debug_build(ui);
                ui.strong("Bottom panel");
            })
        });

        egui::SidePanel::left("left_panel")
            .default_width(500.0)
            .min_width(100.0)
            .frame(egui::Frame {
                fill: egui_ctx.style().visuals.panel_fill,
                ..Default::default()
            })
            .show(egui_ctx, |ui| {
                egui::TopBottomPanel::top("left_panel_tio_bar")
                    .exact_height(0.0)
                    .frame(egui::Frame {
                        inner_margin: 12.0.into(),
                        ..Default::default()
                    })
                    .show_inside(ui, |ui| {
                        ui.horizontal_centered(|ui| {
                            ui.strong("Left bar");
                        });

                        if ui.button("Save Image").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("image", &["png"])
                                .save_file()
                            {
                                pollster::block_on(self.custom.save(path));
                            }
                        }
                    });
            });

        let panel_frame = egui::Frame {
            fill: egui_ctx.style().visuals.panel_fill,
            inner_margin: 12.0.into(),
            ..Default::default()
        };

        egui::SidePanel::right("right_panel")
            .min_width(100.0)
            .frame(panel_frame)
            .show(egui_ctx, |ui| {
                ui.strong("Right panel");
                selection_buttons(ui);
            });

        egui::CentralPanel::default()
            .frame(egui::Frame {
                fill: egui_ctx.style().visuals.panel_fill,
                ..Default::default()
            })
            .show(egui_ctx, |ui| {
                self.custom.custom_painting(ui, frame);
            });
    }
}

impl ExampleApp {
    fn top_bar(&mut self, egui_ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let frame = egui::Frame {
            fill: egui_ctx.style().visuals.panel_fill,
            inner_margin: 12.0.into(),
            ..Default::default()
        };

        egui::TopBottomPanel::top("top_bar")
            .frame(frame)
            .exact_height(24.0)
            .show(egui_ctx, |ui| {
                let _response = egui::menu::bar(ui, |ui| {
                    ui.set_height(24.0);
                    ui.add_space(0.0);
                })
                .response;
            });
    }
}

fn selection_buttons(ui: &mut egui::Ui) {
    use egui_extras::{Size, StripBuilder};

    const BUTTON_SIZE: f32 = 20.0;
    const MIN_COMBOBOX_SIZE: f32 = 100.0;

    ui.horizontal(|ui| {
        StripBuilder::new(ui)
            .cell_layout(egui::Layout::centered_and_justified(
                egui::Direction::TopDown, // whatever
            ))
            .size(Size::exact(BUTTON_SIZE)) // prev
            .size(Size::remainder().at_least(MIN_COMBOBOX_SIZE)) // browser
            .size(Size::exact(BUTTON_SIZE)) // next
            .horizontal(|mut strip| {
                strip.cell(|ui| {
                    let _ = ui.small_button("⏴");
                });

                strip.cell(|ui| {
                    egui::ComboBox::from_id_source("foo")
                        .width(ui.available_width())
                        .selected_text("ComboBox")
                        .show_ui(ui, |ui| {
                            ui.label("contents");
                        });
                });

                strip.cell(|ui| {
                    let _ = ui.small_button("⏵");
                });
            });
    });
}
