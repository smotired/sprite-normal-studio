use eframe::egui;

/// Defines application state
pub struct StudioApp {
    name: String,
    age: u32,
}

/// Default initializer for application state
impl Default for StudioApp {
    fn default() -> Self {
        Self {
            name: String::from("Sam"),
            age: 22,
        }
    }
}

impl eframe::App for StudioApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            // Basic hello world GUI
            ui.heading("Sprite Normal Studio");
            ui.horizontal(|ui| {
                let name_label = ui.label("Your name: ");
                ui.text_edit_singleline(&mut self.name)
                    .labelled_by(name_label.id);
            });
            ui.add(egui::Slider::new(&mut self.age, 0..=120).text("age"));
            if ui.button("Increment").clicked() {
                self.age += 1;
            }
            ui.label(format!("Hello '{}', age {}", self.name, self.age));
        });
    }
}