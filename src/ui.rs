use crate::{
    config::{write_config_file, ClientConfig},
    core::{
        api::{lookup_server, LookupData, LookupError},
        reqwest::Client,
        servers::{has_server_tasks, stop_server_tasks},
    },
    servers::{reset_tunnel_state, start_all_servers, tunnel_state},
    threads::resume_all_threads,
    tunnel_state::{TunnelState, TunnelStateRef},
    update,
};
use futures::FutureExt;
use native_windows_derive::NwgUi;
use native_windows_gui::{init as nwg_init, *};
use parking_lot::Mutex;
use pocket_relay_client_shared::ctx::ClientContext;
use std::{cell::RefCell, sync::Arc};
use tokio::task::JoinHandle;

/// Size of the created window
pub const WINDOW_SIZE: (i32, i32) = (500, 260);
/// Title used for the created window
pub const WINDOW_TITLE: &str = concat!("Pocket Relay Client v", env!("CARGO_PKG_VERSION"));
/// Window icon bytes
pub const ICON_BYTES: &[u8] = include_bytes!("resources/icon.ico");
/// Green status dot icon bytes
pub const DOT_GREEN_BYTES: &[u8] = include_bytes!("resources/dot_green.ico");
/// Yellow status dot icon bytes
pub const DOT_YELLOW_BYTES: &[u8] = include_bytes!("resources/dot_yellow.ico");
/// Red status dot icon bytes
pub const DOT_RED_BYTES: &[u8] = include_bytes!("resources/dot_red.ico");

/// Native GUI app
#[derive(NwgUi, Default)]
pub struct App {
    /// Window Icon
    #[nwg_resource(source_bin: Some(ICON_BYTES))]
    icon: Icon,

    /// App window
    #[nwg_control(
        size: WINDOW_SIZE,
        position: (5, 5),
        icon: Some(&data.icon),
        title: WINDOW_TITLE,
        flags: "WINDOW|VISIBLE|MINIMIZE_BOX"
    )]
    #[nwg_events(OnWindowClose: [stop_thread_dispatch()], OnKeyEnter: [App::handle_set])]
    window: Window,

    /// Green status dot icon
    #[nwg_resource(source_bin: Some(DOT_GREEN_BYTES))]
    dot_green: Icon,

    /// Yellow status dot icon
    #[nwg_resource(source_bin: Some(DOT_YELLOW_BYTES))]
    dot_yellow: Icon,

    /// Red status dot icon
    #[nwg_resource(source_bin: Some(DOT_RED_BYTES))]
    dot_red: Icon,

    /// Grid layout for all the content
    #[nwg_layout(parent: window, max_column: Some(24))]
    grid: GridLayout,

    /// Label for the connection URL input
    #[nwg_control(text: "Please put the server Connection URL below and press 'Set'")]
    #[nwg_layout_item(layout: grid, col: 0, row: 0, col_span: 24)]
    target_url_label: Label,

    /// Input for the connection URL
    #[nwg_control(focus: true)]
    #[nwg_layout_item(layout: grid, col: 0, row: 1, col_span: 16)]
    target_url_input: TextInput,

    /// Button for connecting
    #[nwg_control(text: "Connect")]
    #[nwg_layout_item(layout: grid, col: 16, row: 1, col_span: 8)]
    #[nwg_events(OnButtonClick: [App::handle_set])]
    set_button: Button,

    /// Checkbox for whether to remember the connection URL
    #[nwg_control(text: "Save connection URL")]
    #[nwg_layout_item(layout: grid, col: 0, row: 2, col_span: 12)]
    remember_checkbox: CheckBox,

    /// Checkbox for whether to automatically connect on startup
    #[nwg_control(text: "Auto connect on startup")]
    #[nwg_layout_item(layout: grid, col: 12, row: 2, col_span: 12)]
    auto_connect_checkbox: CheckBox,

    /// Server status dot
    #[nwg_control(icon: Some(&data.dot_red))]
    #[nwg_layout_item(layout: grid, col: 0, row: 3, col_span: 2)]
    server_dot: ImageFrame,

    /// Connection state label
    #[nwg_control(text: "Server: Not connected")]
    #[nwg_layout_item(layout: grid, col: 2, row: 3, col_span: 22)]
    connection_label: Label,

    /// Tunnel status dot
    #[nwg_control(icon: Some(&data.dot_red))]
    #[nwg_layout_item(layout: grid, col: 0, row: 4, col_span: 2)]
    tunnel_dot: ImageFrame,

    /// Tunnel state label (empty when the server has no tunneling)
    #[nwg_control(text: "Tunnel: Not available")]
    #[nwg_layout_item(layout: grid, col: 2, row: 4, col_span: 22)]
    tunnel_label: Label,

    /// Label about connecting
    #[nwg_control(text: "Your game will start after you connect. If you don't want to connect to
 a Pocket Relay server close this window and you will connect to the
 official servers")]
    #[nwg_layout_item(layout: grid, col: 0, row: 5, col_span: 24, row_span: 3)]
    connect_label: Label,

    /// Notice for tunnel state changes
    #[nwg_control]
    #[nwg_events(OnNotice: [App::handle_tunnel_notice])]
    tunnel_notice: Notice,

    /// Notice for connection completion
    #[nwg_control]
    #[nwg_events(OnNotice: [App::handle_connect_notice])]
    connect_notice: Notice,

    /// Join handle for the connect task
    connect_task: RefCell<Option<JoinHandle<Result<LookupData, LookupError>>>>,

    /// Http client for sending requests
    http_client: Client,

    /// The current shared tunnel state
    tunnel_state: Arc<Mutex<TunnelState>>,
}

impl App {
    /// Handles the "Set" button being pressed, dispatches a connect task
    /// that will wake up the App with `App::handle_connect_notice` to
    /// handle the connection result.
    fn handle_set(&self) {
        // Abort any existing connection tasks
        if let Some(task) = self.connect_task.take() {
            task.abort();
        }

        // Handle disconnecting
        if has_server_tasks() {
            stop_server_tasks();

            self.reset_tunnel_state();
            self.update_tunnel_status();

            self.connection_label.set_text("Server: Not connected");
            self.server_dot.set_icon(Some(&self.dot_red));
            self.set_button.set_text("Connect");
            return;
        }

        self.connection_label.set_text("Server: Connecting...");
        let target = self.target_url_input.text().to_string();
        let sender = self.connect_notice.sender();
        let http_client = self.http_client.clone();

        let task = tokio::spawn(async move {
            let result = lookup_server(http_client, target).await;
            sender.notice();
            result
        });

        *self.connect_task.borrow_mut() = Some(task);
    }

    /// Handles the tunnel task changing state
    fn handle_tunnel_notice(&self) {
        self.update_tunnel_status();
    }

    /// gets the current tunnel state
    fn get_tunnel_state(&self) -> TunnelState {
        self.tunnel_state.lock()
    }

    /// Resets the tunnel state to its initial value
    fn reset_tunnel_state(&self) {
        *self.tunnel_state.lock() = TunnelState::default();
    }

    /// Updates the tunnel dot and label from the current tunnel state
    fn update_tunnel_status(&self) {
        let tunnel_state = self.get_tunnel_state();

        let (icon, text) = match tunnel_state.get_state() {
            TunnelState::NotConnected => (&self.dot_yellow, "Tunnel: Preparing"),
            TunnelState::Creating => (&self.dot_yellow, "Tunnel: Connecting"),
            TunnelState::Handshaking => (&self.dot_yellow, "Tunnel: Connecting (UDP)"),
            TunnelState::RetryConnect => (&self.dot_yellow, "Tunnel: Retrying connection"),
            //
            TunnelState::Connected => (&self.dot_green, "Tunnel: Connected"),
            //
            TunnelState::Error => (&self.dot_red, "Tunnel: Disconnected"),
            TunnelState::Unavailable => (&self.dot_red, "Tunnel: Not available"),
        };
        self.tunnel_dot.set_icon(Some(icon));
        self.tunnel_label.set_text(text);
    }

    /// Handles the connection complete notice updating the UI
    /// with the new connection state from the task result
    fn handle_connect_notice(&self) {
        let result = self
            .connect_task
            .borrow_mut()
            .take()
            // Flatten on the join result
            .and_then(FutureExt::now_or_never)
            // Flatten join failure errors (Out of our control)
            .and_then(Result::ok);

        // Ensure theres actually a result to use
        let Some(result) = result else { return };

        let mut lookup = match result {
            Ok(value) => value,
            Err(err) => {
                self.connection_label.set_text("Server: Failed to connect");
                error_message("Failed to connect", &err.to_string());
                return;
            }
        };

        let ctx = Arc::new(ClientContext {
            http_client: self.http_client.clone(),
            base_url: lookup.url.clone(),
            association: lookup.association.take(),
            tunnel_port: lookup.tunnel_port,
        });

        // Initialize a tunnel state
        let tunnel_state =
            TunnelStateRef::new(self.tunnel_state.clone(), self.tunnel_notice.sender());

        // Start the servers
        start_all_servers(ctx, tunnel_state);

        let remember = self.remember_checkbox.check_state() == CheckBoxState::Checked;
        let auto_connect = self.auto_connect_checkbox.check_state() == CheckBoxState::Checked;

        // Save the connection URL (auto connect requires a saved URL)
        if remember || auto_connect {
            let connection_url = lookup.url.to_string();
            write_config_file(ClientConfig {
                connection_url,
                auto_connect,
            });
        }

        let text = format!(
            "Server: Connected - {} {} version v{}",
            lookup.url.scheme(),
            lookup.url.authority(),
            lookup.version
        );
        self.connection_label.set_text(&text);
        self.server_dot.set_icon(Some(&self.dot_green));
        self.update_tunnel_status();
        self.set_button.set_text("Disconnect");

        // Resume game threads
        resume_all_threads();
    }
}

/// Initializes the user interface
///
/// ## Arguments
/// * `config` - The client config to use
/// * `client` - The HTTP client to use
pub fn init(config: Option<ClientConfig>, client: Client) {
    // Create tokio async runtime
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed building tokio runtime");

    // Enter the tokio runtime
    let _enter = runtime.enter();

    // Spawn the updating task
    tokio::spawn(update::update(client.clone()));

    // Initialize nwg
    nwg_init().expect("Failed to initialize native UI");

    // Set the default font family
    Font::set_global_family("Segoe UI").expect("Failed to set default font");

    // Build the app UI
    let app = App::build_ui(App {
        http_client: client,
        ..Default::default()
    })
    .expect("Failed to build native UI");

    let (target, remember, auto_connect) = config
        .map(|value| (value.connection_url, true, value.auto_connect))
        .unwrap_or_default();

    app.target_url_input.set_text(&target);

    if remember {
        app.remember_checkbox
            .set_check_state(CheckBoxState::Checked);
    }

    if auto_connect {
        app.auto_connect_checkbox
            .set_check_state(CheckBoxState::Checked);

        // Connect right away to the saved server
        if !target.is_empty() {
            app.handle_set();
        }
    }

    dispatch_thread_events();

    // Resume the game threads if we close the UI
    resume_all_threads();

    let shutdown_signal = tokio::signal::ctrl_c();
    let _ = runtime.block_on(shutdown_signal);
}

/// Shows a confirmation message to the user returning
/// the choice that the user made.
///
/// ## Arguments
/// * `title` - The title for the dialog
/// * `text`  - The text for the dialog
pub fn confirm_message(title: &str, text: &str) -> bool {
    let choice = message(&MessageParams {
        title,
        content: text,
        buttons: MessageButtons::YesNo,
        icons: MessageIcons::Question,
    });

    matches!(choice, MessageChoice::Yes)
}

/// Shows a info message to the user.
///
/// ## Arguments
/// * `title` - The title for the dialog
/// * `text`  - The text for the dialog
pub fn info_message(title: &str, text: &str) {
    message(&MessageParams {
        title,
        content: text,
        buttons: MessageButtons::Ok,
        icons: MessageIcons::Info,
    });
}

/// Shows an error message to the user.
///
/// ## Arguments
/// * `title` - The title for the dialog
/// * `text`  - The text for the dialog
pub fn error_message(title: &str, text: &str) {
    message(&MessageParams {
        title,
        content: text,
        buttons: MessageButtons::Ok,
        icons: MessageIcons::Error,
    });
}
