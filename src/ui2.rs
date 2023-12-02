use hudhook::hooks::ImguiRenderLoop;
use imgui::*;
use log::debug;
use std::time::Instant;

pub struct GameUi {
    window_active: bool,
    connection_url: String,
    remember: bool,
}

impl GameUi {
    pub fn new() -> Self {
        Self {
            window_active: false,
            connection_url: String::new(),
            remember: false,
        }
    }
}

impl Default for GameUi {
    fn default() -> Self {
        Self::new()
    }
}

impl ImguiRenderLoop for GameUi {
    fn render(&mut self, ui: &mut Ui) {
        ui.window("##hello")
            .draw_background(false)
            .title_bar(false)
            .collapsible(false)
            .build(|| {
                self.window_active = !ui.is_window_collapsed()
                    && (ui
                        .is_window_hovered_with_flags(WindowHoveredFlags::ROOT_AND_CHILD_WINDOWS)
                        || ui.is_window_focused_with_flags(
                            WindowFocusedFlags::ROOT_AND_CHILD_WINDOWS,
                        ));

                self.window_active = ui.is_window_hovered() && !ui.is_window_collapsed();

                _ = ui
                    .input_text("Connection URL", &mut self.connection_url)
                    .build();

                if ui.button("Connection") {
                    debug!("Connecting");
                }

                _ = ui.checkbox("Save connection URL", &mut self.remember);

                ui.text("Connected to 127.0.0.1");
            });
    }

    fn should_block_messages(&self, io: &Io) -> bool {
        // Handle imgui using the UI
        if io.want_capture_mouse || io.want_capture_keyboard || io.want_text_input {
            return true;
        }

        false
    }
}
