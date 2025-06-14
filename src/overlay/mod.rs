use std::{sync::Arc, time::Duration};

use hudhook::{imgui::StyleColor, *};
use parking_lot::Mutex;
use pocket_relay_client_shared::{
    api::{lookup_server, LookupData},
    ctx::ClientContext,
    reqwest::Client,
    servers::{has_server_tasks, stop_server_tasks},
};
use tokio::{runtime::Runtime, task::AbortHandle};

use crate::{
    config::{write_config_file, ClientConfig},
    servers::start_all_servers,
};

pub struct OverlayRenderLoop {
    screen: OverlayScreen,

    initial_startup_screen: InitialStartupScreen,

    connection_state: Arc<Mutex<ConnectionState>>,

    /// Http client for sending requests
    http_client: Client,

    runtime: Runtime,
}

impl OverlayRenderLoop {
    pub fn new(runtime: Runtime, config: Option<ClientConfig>, http_client: Client) -> Self {
        Self {
            screen: OverlayScreen::InitialStartup,
            initial_startup_screen: InitialStartupScreen {
                remember_url: config.is_some(),
                target_url: config.map(|value| value.connection_url).unwrap_or_default(),
                connect_task: None,
            },
            connection_state: Default::default(),
            http_client,
            runtime,
        }
    }
}

impl ImguiRenderLoop for OverlayRenderLoop {
    fn render(&mut self, ui: &mut imgui::Ui) {
        match self.screen {
            OverlayScreen::InitialStartup => render_startup_screen(self, ui),
            OverlayScreen::Game => {
                let io = ui.io();

                if io.key_ctrl && io.key_shift && io.keys_down[imgui::Key::P as usize] {
                    self.screen = OverlayScreen::GameOverlay;
                }
            }
            OverlayScreen::GameOverlay => {
                let io = ui.io();

                if io.keys_down[imgui::Key::Escape as usize] {
                    self.screen = OverlayScreen::Game;
                }

                render_game_overlay(self, ui)
            }
        }
    }

    fn message_filter(&self, io: &imgui::Io) -> MessageFilter {
        let mut filter = MessageFilter::empty();

        match &self.screen {
            OverlayScreen::InitialStartup | OverlayScreen::GameOverlay => {
                if io.want_capture_mouse {
                    filter |= MessageFilter::InputMouse;
                    filter |= MessageFilter::InputKeyboard;
                }
                if io.want_capture_keyboard {
                    filter |= MessageFilter::InputKeyboard;
                }
            }
            OverlayScreen::Game => {}
        }

        filter
    }
}

#[derive(Clone, Copy)]
pub enum OverlayScreen {
    /// Game has just started, waiting for the user to decide whether
    /// they want to connect or not
    InitialStartup,

    /// User is playing the game, don't show anything
    Game,

    /// User has opened the game overlay manually
    GameOverlay,
}

pub struct InitialStartupScreen {
    /// Current URL the user has put in
    target_url: String,

    /// Whether to remember the URL for the next
    /// game startup
    remember_url: bool,

    /// Background task for connecting
    connect_task: Option<AbortHandle>,
}

#[derive(Default)]
pub enum ConnectionState {
    #[default]
    Initial,
    Connecting,
    Connected(LookupData),
    Error(String),
}

fn overlay_window(ui: &mut imgui::Ui, display_size: [f32; 2]) {
    ui.window("##background_overlay")
        .no_decoration()
        .title_bar(false)
        .movable(false)
        .resizable(false)
        .collapsible(false)
        .bring_to_front_on_focus(false)
        .bg_alpha(0.7)
        .position([0.0, 0.0], imgui::Condition::Always)
        .size(display_size, imgui::Condition::Always)
        .scroll_bar(false)
        .scrollable(false)
        .build(|| {});
}

pub fn render_game_overlay(parent: &mut OverlayRenderLoop, ui: &mut imgui::Ui) {
    let display_size = ui.io().display_size;
    overlay_window(ui, display_size);

    ui.window("Pocket Relay")
        .resizable(false)
        .size([450.0, 350.0], imgui::Condition::Always)
        .build(|| {
            let is_connected;
            let allowed_connect;

            {
                let state = &*parent.connection_state.lock();
                allowed_connect = !matches!(state, ConnectionState::Connecting);
                is_connected = matches!(state, ConnectionState::Connected(_));
                status_text(ui, state);
            }

            if is_connected {
                let disconnect_pressed = ui.button("Disconnect");
                if disconnect_pressed {
                    on_click_disconnect(parent);
                }
            } else {
                let connect_pressed = connect_button(ui, allowed_connect);
                if connect_pressed {
                    on_click_connect(parent);
                }
            }
        });
}

pub fn render_startup_screen(parent: &mut OverlayRenderLoop, ui: &mut imgui::Ui) {
    let display_size = ui.io().display_size;

    overlay_window(ui, display_size);

    let window_size = [450.0, 350.0];
    let window_pos = [
        (display_size[0] - window_size[0]) * 0.5,
        (display_size[1] - window_size[1]) * 0.5,
    ];

    let window = ui
        .window("Pocket Relay Introduction")
        .no_decoration()
        .title_bar(false)
        .movable(false)
        .resizable(false)
        .position(window_pos, imgui::Condition::Always)
        .size(window_size, imgui::Condition::Always)
        .collapsible(false);

    if let Some(_window_token) = window.begin() {
        ui.text("Pocket Relay client");
        ui.text("Please put the server Connection URL below");

        ui.input_text("Server URL", &mut parent.initial_startup_screen.target_url)
            .hint("Enter the server address")
            .build();

        ui.checkbox(
            "Save Connection URL",
            &mut parent.initial_startup_screen.remember_url,
        );

        ui.text_wrapped("If you don't want to connect to a Pocket Relay server press 'Cancel'");

        let allowed_connect;

        {
            let state = &*parent.connection_state.lock();
            allowed_connect = !matches!(state, ConnectionState::Connecting);
            status_text(ui, state);
        };

        let connect_pressed = connect_button(ui, allowed_connect);

        ui.same_line();

        let cancel_pressed = ui.button("Cancel");

        if cancel_pressed {
            on_click_cancel(parent);
        }

        if connect_pressed {
            on_click_connect(parent);
        }
    }
}

const DISABLED_BUTTON_COLOR: [f32; 4] = [0.3, 0.3, 0.3, 1.0];

fn status_text(ui: &imgui::Ui, state: &ConnectionState) {
    match state {
        ConnectionState::Initial => ui.text("Not connected."),
        ConnectionState::Connecting => ui.text("Connecting..."),
        ConnectionState::Connected(data) => {
            ui.text("Connected:");
            ui.same_line();
            ui.text_wrapped(data.url.as_str());
            ui.same_line();
            ui.text_wrapped(" version ");
            ui.same_line();
            ui.text_wrapped(data.version.to_string());
        }
        ConnectionState::Error(error) => {
            ui.text_wrapped("Failed to connect");
            ui.same_line();
            ui.text_wrapped(error);
        }
    }
}

fn connect_button(ui: &imgui::Ui, allowed_connect: bool) -> bool {
    let (button_color, button_hovered_color, button_active_color) = if allowed_connect {
        (
            [0.2, 0.5, 1.0, 1.0],
            [0.3, 0.6, 1.0, 1.0],
            [0.1, 0.4, 0.9, 1.0],
        )
    } else {
        (
            DISABLED_BUTTON_COLOR,
            DISABLED_BUTTON_COLOR,
            DISABLED_BUTTON_COLOR,
        )
    };

    let _bc = ui.push_style_color(StyleColor::Button, button_color);
    let _bhc = ui.push_style_color(StyleColor::ButtonHovered, button_hovered_color);
    let _bac = ui.push_style_color(StyleColor::ButtonActive, button_active_color);
    ui.button("Connect")
}

fn on_click_cancel(parent: &mut OverlayRenderLoop) {
    parent.screen = OverlayScreen::Game;
}

fn on_click_connect(parent: &mut OverlayRenderLoop) {
    // Abort existing task
    if let Some(abort_handle) = parent.initial_startup_screen.connect_task.take() {
        abort_handle.abort();
    }

    let state = parent.connection_state.clone();
    let url = parent.initial_startup_screen.target_url.clone();
    let http_client = parent.http_client.clone();
    let remember = parent.initial_startup_screen.remember_url;

    {
        *state.lock() = ConnectionState::Connecting;
    }

    // Run lookup task
    let abort_handle = parent
        .runtime
        .spawn(async move {
            let result = lookup_server(http_client.clone(), url).await;

            tokio::time::sleep(Duration::from_secs(5)).await;
            match result {
                Ok(mut lookup) => {
                    let ctx = Arc::new(ClientContext {
                        http_client,
                        base_url: lookup.url.clone(),
                        association: lookup.association.take(),
                        tunnel_port: lookup.tunnel_port,
                    });

                    // Start the servers
                    start_all_servers(ctx);

                    // Save the connection URL
                    if remember {
                        let connection_url = lookup.url.to_string();
                        write_config_file(ClientConfig { connection_url });
                    }

                    {
                        *state.lock() = ConnectionState::Connected(lookup.clone());
                    }
                }
                Err(value) => {
                    *state.lock() = ConnectionState::Error(value.to_string());
                }
            }
        })
        .abort_handle();

    parent.initial_startup_screen.connect_task = Some(abort_handle);
}

fn on_click_disconnect(parent: &mut OverlayRenderLoop) {
    // Abort existing task
    if let Some(abort_handle) = parent.initial_startup_screen.connect_task.take() {
        abort_handle.abort();
    }

    // Handle disconnecting
    if has_server_tasks() {
        stop_server_tasks();
    }

    *parent.connection_state.lock() = ConnectionState::Initial;
}

pub struct GameScreen;
