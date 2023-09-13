use crate::{
    config::ClientConfig,
    constants::{APP_VERSION, ICON_BYTES},
    servers::{servers_running_blocking, stop_server_tasks, try_start_servers},
};
use log::{debug, error};
use ngw::{CheckBoxState, GridLayoutItem, Icon};

extern crate native_windows_gui as ngw;

pub const WINDOW_SIZE: (i32, i32) = (500, 135);

pub fn init(runtime: tokio::runtime::Handle, config: Option<ClientConfig>) {
    ngw::init().expect("Failed to initialize native UI");
    ngw::Font::set_global_family("Segoe UI").expect("Failed to set default font");

    let (target, remember) = config
        .map(|value| (value.connection_url, true))
        .unwrap_or_default();

    let mut window = Default::default();
    let mut target_url = Default::default();
    let mut set_button = Default::default();
    let mut remember_checkbox = Default::default();
    let layout = Default::default();

    let mut top_label = Default::default();
    let mut c_label = Default::default();

    let mut icon = Default::default();

    Icon::builder()
        .source_bin(Some(ICON_BYTES))
        .build(&mut icon)
        .unwrap();

    // Create window
    ngw::Window::builder()
        .size(WINDOW_SIZE)
        .position((5, 5))
        .icon(Some(&icon))
        .title(&format!("Pocket Relay Client Plugin v{}", APP_VERSION))
        .build(&mut window)
        .unwrap();

    // Create information text
    ngw::Label::builder()
        .text("Please put the server Connection URL below and press 'Connect'")
        .parent(&window)
        .build(&mut top_label)
        .unwrap();

    ngw::Label::builder()
        .text("Not connected")
        .parent(&window)
        .build(&mut c_label)
        .unwrap();

    // Create the url input and set button
    ngw::TextInput::builder()
        .text(&target)
        .focus(true)
        .parent(&window)
        .build(&mut target_url)
        .unwrap();
    ngw::Button::builder()
        .text("Connect")
        .parent(&window)
        .build(&mut set_button)
        .unwrap();
    ngw::CheckBox::builder()
        .text("Save connection URL")
        .check_state(if remember {
            CheckBoxState::Checked
        } else {
            CheckBoxState::Unchecked
        })
        .parent(&window)
        .build(&mut remember_checkbox)
        .unwrap();

    // Create the layout grid for the UI
    ngw::GridLayout::builder()
        .parent(&window)
        .spacing(0)
        .child_item(GridLayoutItem::new(&top_label, 0, 0, 5, 1))
        .child_item(GridLayoutItem::new(&target_url, 0, 1, 4, 1))
        .child_item(GridLayoutItem::new(&set_button, 4, 1, 1, 1))
        .child_item(GridLayoutItem::new(&remember_checkbox, 0, 2, 5, 1))
        .child_item(GridLayoutItem::new(&c_label, 0, 3, 5, 1))
        .build(&layout)
        .unwrap();

    let window_handle = window.handle;

    let handler = ngw::full_bind_event_handler(&window_handle, move |event, _evt_data, handle| {
        use ngw::Event as E;

        match event {
            E::OnWindowClose => {
                if handle == window_handle {
                    ngw::stop_thread_dispatch();
                }
            }

            E::OnButtonClick => {
                if handle == set_button {
                    if servers_running_blocking() {
                        c_label.set_text("Disconnecting...");

                        runtime.block_on(stop_server_tasks());

                        c_label.set_text("Not connected");
                        set_button.set_text("Connect")
                    } else {
                        c_label.set_text("Connecting...");

                        let target = target_url.text();
                        let value = match runtime.block_on(try_start_servers(
                            target,
                            remember_checkbox.check_state() == CheckBoxState::Checked,
                        )) {
                            Ok(value) => value,
                            Err(err) => {
                                c_label.set_text("Failed to connect");
                                ngw::modal_error_message(
                                    window_handle,
                                    "Failed to connect",
                                    &err.to_string(),
                                );
                                error!("Failed to connect: {}", err);
                                return;
                            }
                        };

                        debug!(
                            "Connected to server {} {} version v{}",
                            value.scheme, value.host, value.version
                        );

                        let message = format!(
                            "Connected: {} {} version v{}",
                            value.scheme, value.host, value.version
                        );

                        c_label.set_text(&message);
                        set_button.set_text("Disconnect")
                    }
                }
            }
            _ => {}
        }
    });

    ngw::dispatch_thread_events();
    ngw::unbind_event_handler(&handler);
}
