use crate::{
    config::{write_config_file, ClientConfig},
    constants::{ICON_BYTES, WINDOW_TITLE},
    servers::start_all_servers,
    update,
};
use futures::FutureExt;
use ngd::NwgUi;
use nwg::{error_message, CheckBoxState, NativeUi};
use pocket_relay_client_shared::{
    api::{lookup_server, LookupData, LookupError},
    reqwest::Client,
    servers::{has_server_tasks, stop_server_tasks},
};
use std::cell::RefCell;
use tokio::task::JoinHandle;

extern crate native_windows_derive as ngd;
extern crate native_windows_gui as nwg;

pub const WINDOW_SIZE: (i32, i32) = (500, 135);

#[derive(NwgUi, Default)]
pub struct App {
    /// Window Icon
    #[nwg_resource(source_bin: Some(ICON_BYTES))]
    icon: nwg::Icon,

    /// App window
    #[nwg_control(
        size: WINDOW_SIZE,
        position: (5, 5),
        icon: Some(&data.icon),
        title: WINDOW_TITLE,
        flags: "WINDOW|VISIBLE|MINIMIZE_BOX"
    )]
    #[nwg_events(OnWindowClose: [nwg::stop_thread_dispatch()])]
    window: nwg::Window,

    /// Grid layout for all the content
    #[nwg_layout(parent: window)]
    grid: nwg::GridLayout,

    /// Label for the connection URL input
    #[nwg_control(text: "Please put the server Connection URL below and press 'Set'")]
    #[nwg_layout_item(layout: grid, col: 0, row: 0, col_span: 2)]
    target_url_label: nwg::Label,

    /// Input for the connection URL
    #[nwg_control(focus: true)]
    #[nwg_layout_item(layout: grid, col: 0, row: 1, col_span: 2)]
    target_url_input: nwg::TextInput,

    /// Button for connecting
    #[nwg_control(text: "Set")]
    #[nwg_layout_item(layout: grid, col: 2, row: 1, col_span: 1)]
    #[nwg_events(OnButtonClick: [App::handle_set])]
    set_button: nwg::Button,

    /// Checkbox for whether to remember the connection URL
    #[nwg_control(text: "Save connection URL")]
    #[nwg_layout_item(layout: grid, col: 0, row: 2, col_span: 3)]
    remember_checkbox: nwg::CheckBox,

    /// Connection state label
    #[nwg_control(text: "Not connected")]
    #[nwg_layout_item(layout: grid, col: 0, row: 3, col_span: 3)]
    connection_label: nwg::Label,

    /// Notice for connection completion
    #[nwg_control]
    #[nwg_events(OnNotice: [App::handle_connect_notice])]
    connect_notice: nwg::Notice,

    /// Join handle for the connect task
    connect_task: RefCell<Option<JoinHandle<Result<LookupData, LookupError>>>>,

    /// Http client for sending requests
    http_client: Client,
}

impl App {
    /// Handles the "Set" button being pressed, dispatches a connect task
    /// that will wake up the App with `App::handle_connect_notice` to
    /// handle the connection result.
    fn handle_set(&self) {
        if let Some(task) = self.connect_task.take() {
            task.abort();
        }

        if has_server_tasks() {
            stop_server_tasks();
            self.connection_label.set_text("Not connected");
            self.set_button.set_text("Connect");
            return;
        }

        self.connection_label.set_text("Connecting...");
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

    /// Handles the connection complete notice updating the UI
    /// with the new connection state from the task result
    fn handle_connect_notice(&self) {
        let result = self
            .connect_task
            .borrow_mut()
            .take()
            // Flatten on the join result
            .and_then(|task| task.now_or_never())
            // Flatten join failure errors (Out of our control)
            .and_then(|inner| inner.ok());

        let result = match result {
            Some(value) => value,
            None => {
                return;
            }
        };

        match result {
            Ok(result) => {
                // Start the servers
                start_all_servers(self.http_client.clone(), result.url.clone());

                let remember = self.remember_checkbox.check_state() == CheckBoxState::Checked;

                // Save the connection URL
                if remember {
                    let connection_url = result.url.to_string();
                    tokio::spawn(async move {
                        write_config_file(ClientConfig { connection_url }).await;
                    });
                }

                let text = format!(
                    "Connected: {} {} version v{}",
                    result.url.scheme(),
                    result.url.authority(),
                    result.version
                );
                self.connection_label.set_text(&text)
            }
            Err(err) => {
                self.connection_label.set_text("Failed to connect");
                error_message("Failed to connect", &err.to_string());
            }
        }
    }
}

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

    nwg::init().expect("Failed to initialize native UI");
    nwg::Font::set_global_family("Segoe UI").expect("Failed to set default font");

    let app = App::build_ui(App {
        http_client: client,
        ..Default::default()
    })
    .expect("Failed to build native UI");

    let (target, remember) = config
        .map(|value| (value.connection_url, true))
        .unwrap_or_default();

    app.target_url_input.set_text(&target);

    if remember {
        app.remember_checkbox
            .set_check_state(CheckBoxState::Checked);
    }

    nwg::dispatch_thread_events();

    // Block for CTRL+C to keep servers alive when window closes
    let shutdown_signal = tokio::signal::ctrl_c();
    let _ = runtime.block_on(shutdown_signal);
}

pub fn show_confirm(title: &str, text: &str) -> bool {
    let params = native_windows_gui::MessageParams {
        title,
        content: text,
        buttons: native_windows_gui::MessageButtons::YesNo,
        icons: native_windows_gui::MessageIcons::Question,
    };

    native_windows_gui::message(&params) == native_windows_gui::MessageChoice::Yes
}