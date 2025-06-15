use crate::{
    config::{write_config_file, ClientConfig},
    servers::start_all_servers,
};
use hudhook::{
    imgui::{self, Image, StyleColor},
    windows::Win32::UI::WindowsAndMessaging::SetCursor,
    ImguiRenderLoop, MessageFilter,
};
use image::{EncodableLayout, ImageReader, RgbaImage};
use imgui::TextureId;
use parking_lot::Mutex;
use pocket_relay_client_shared::{
    api::{lookup_server, LookupData},
    ctx::ClientContext,
    reqwest::Client,
    servers::{has_server_tasks, stop_server_tasks},
};
use std::{io::Cursor, sync::Arc};
use tokio::{
    runtime::Runtime,
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    task::AbortHandle,
};

pub enum GameEventMessage {
    GameStartupComplete,

    UpdateConnectionState(ConnectionState),
}

pub static mut GAME_EVENT_SENDER: Option<UnboundedSender<GameEventMessage>> = None;

pub struct OverlayRenderLoop {
    screen: OverlayScreen,

    initial_startup_screen: InitialStartupScreen,

    connection_state: ConnectionState,

    /// Http client for sending requests
    http_client: Client,

    runtime: Runtime,

    logo_image: Option<RgbaImage>,
    logo_image_id: Option<TextureId>,

    messages_rx: UnboundedReceiver<GameEventMessage>,
}

impl OverlayRenderLoop {
    pub fn new(runtime: Runtime, config: Option<ClientConfig>, http_client: Client) -> Self {
        let logo_image = ImageReader::with_format(
            Cursor::new(include_bytes!("../../assets/logo-dark.png")),
            image::ImageFormat::Png,
        )
        .decode()
        .expect("failed to decode logo image")
        .into_rgba8();

        let (tx, messages_rx) = unbounded_channel();

        unsafe {
            GAME_EVENT_SENDER = Some(tx);
        };

        Self {
            screen: OverlayScreen::PreStartup,
            initial_startup_screen: InitialStartupScreen {
                remember_url: config.is_some(),
                target_url: config.map(|value| value.connection_url).unwrap_or_default(),
                connect_task: None,
            },
            connection_state: Default::default(),
            http_client,
            runtime,
            logo_image: Some(logo_image),
            logo_image_id: None,
            messages_rx,
        }
    }
}

pub static IS_IN_BLOCKING_UI: Mutex<bool> = Mutex::new(false);

impl OverlayRenderLoop {
    fn set_screen(&mut self, screen: OverlayScreen) {
        {
            *IS_IN_BLOCKING_UI.lock() = matches!(
                screen,
                OverlayScreen::GameOverlay | OverlayScreen::InitialStartup
            );
        }

        self.screen = screen;
    }
}

impl ImguiRenderLoop for OverlayRenderLoop {
    fn initialize<'a>(
        &'a mut self,
        _ctx: &mut imgui::Context,
        render_context: &'a mut dyn hudhook::RenderContext,
    ) {
        // Load the logo image
        if let Some(logo_image) = self.logo_image.take() {
            if let Ok(logo_image_id) = render_context.load_texture(
                logo_image.as_bytes(),
                logo_image.width(),
                logo_image.height(),
            ) {
                self.logo_image_id = Some(logo_image_id);
            }
        }
    }

    fn before_render<'a>(
        &'a mut self,
        ctx: &mut imgui::Context,
        _render_context: &'a mut dyn hudhook::RenderContext,
    ) {
        let io = ctx.io_mut();

        match &self.screen {
            OverlayScreen::InitialStartup | OverlayScreen::GameOverlay => {
                io.mouse_draw_cursor = true;
            }

            _ => {
                io.mouse_draw_cursor = false;
            }
        }
    }

    fn render(&mut self, ui: &mut imgui::Ui) {
        // Get messages from other threads for state updates
        while let Ok(msg) = self.messages_rx.try_recv() {
            match msg {
                GameEventMessage::GameStartupComplete => {
                    if matches!(self.screen, OverlayScreen::PreStartup) {
                        self.set_screen(OverlayScreen::InitialStartup);
                    }
                }

                GameEventMessage::UpdateConnectionState(state) => {
                    self.connection_state = state;
                }
            }
        }

        // We are connected move onto the next screen
        if matches!(self.connection_state, ConnectionState::Connected(_))
            && matches!(self.screen, OverlayScreen::InitialStartup)
        {
            self.set_screen(OverlayScreen::Game);
        }

        match self.screen {
            OverlayScreen::PreStartup => {}
            OverlayScreen::InitialStartup => render_startup_screen(self, ui),
            OverlayScreen::Game => {
                let io = ui.io();

                if io.key_ctrl && io.key_shift && io.keys_down[imgui::Key::P as usize] {
                    self.set_screen(OverlayScreen::GameOverlay);
                }
            }
            OverlayScreen::GameOverlay => {
                let io = ui.io();

                // Escape to close overlay
                if io.keys_down[imgui::Key::Escape as usize] {
                    self.set_screen(OverlayScreen::Game);
                }

                render_game_overlay(self, ui)
            }
        }
    }

    fn message_filter(&self, io: &imgui::Io) -> MessageFilter {
        let mut filter = MessageFilter::empty();

        match &self.screen {
            OverlayScreen::PreStartup => {}
            OverlayScreen::InitialStartup | OverlayScreen::GameOverlay => {
                // When the mouse is wanted (Overlay is present), steal all input
                // prevent moving the player around while the overlay is open
                if io.want_capture_mouse {
                    return MessageFilter::InputAll;
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
    /// Game has not hit the splash screen yet, don't show UI yet
    PreStartup,

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
        .movable(false)
        .bring_to_front_on_focus(false)
        .bg_alpha(0.9)
        .position([0.0, 0.0], imgui::Condition::Always)
        .size(display_size, imgui::Condition::Always)
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
            let is_connected = matches!(parent.connection_state, ConnectionState::Connected(_));
            status_text(ui, &parent.connection_state);

            if is_connected {
                let disconnect_pressed = ui.button("Disconnect");
                if disconnect_pressed {
                    on_click_disconnect(parent);
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
        // Render logo
        if let Some(logo_image_id) = parent.logo_image_id {
            let logo_width = 345.0 * 0.4;
            let logo_height = 135.0 * 0.4;

            ui.set_cursor_pos([(window_size[0] - logo_width) * 0.5, ui.cursor_pos()[1]]);

            Image::new(logo_image_id, [logo_width, logo_height]).build(ui);
        }

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

        let allowed_connect = !matches!(parent.connection_state, ConnectionState::Connecting);
        status_text(ui, &parent.connection_state);

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

    let url = parent.initial_startup_screen.target_url.clone();
    let http_client = parent.http_client.clone();
    let remember = parent.initial_startup_screen.remember_url;

    parent.connection_state = ConnectionState::Connecting;

    // Run lookup task
    let abort_handle = parent
        .runtime
        .spawn(async move {
            let result = lookup_server(http_client.clone(), url).await;

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

                    if let Some(sender) = unsafe { &GAME_EVENT_SENDER } {
                        _ = sender.send(GameEventMessage::UpdateConnectionState(
                            ConnectionState::Connected(lookup),
                        ));
                    }
                }
                Err(error) => {
                    if let Some(sender) = unsafe { &GAME_EVENT_SENDER } {
                        _ = sender.send(GameEventMessage::UpdateConnectionState(
                            ConnectionState::Error(error.to_string()),
                        ));
                    }
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

    parent.connection_state = ConnectionState::Initial;
}

pub struct GameScreen;
