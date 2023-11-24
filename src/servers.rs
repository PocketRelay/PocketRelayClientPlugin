use log::error;
use native_windows_gui::error_message;
use pocket_relay_client_shared::{reqwest, servers::*, Url};
use std::sync::Arc;

/// Starts all the servers in their own tasks
pub fn start_all_servers(http_client: reqwest::Client, base_url: Arc<Url>) {
    // Stop existing servers and tasks if they are running
    stop_server_tasks();

    // Spawn the Redirector server
    spawn_server_task(async move {
        if let Err(err) = redirector::start_redirector_server().await {
            error_message("Failed to start redirector server", &err.to_string());
            error!("Failed to start redirector server: {}", err);
        }
    });

    // Need to copy the client and base_url so it can be moved into the task
    let (a, b) = (http_client.clone(), base_url.clone());

    // Spawn the Blaze server
    spawn_server_task(async move {
        if let Err(err) = blaze::start_blaze_server(a, b).await {
            error_message("Failed to start blaze server", &err.to_string());
            error!("Failed to start blaze server: {}", err);
        }
    });

    // Need to copy the client and base_url so it can be moved into the task
    let (a, b) = (http_client.clone(), base_url.clone());

    // Spawn the HTTP server
    spawn_server_task(async move {
        if let Err(err) = http::start_http_server(a, b).await {
            error_message("Failed to start http server", &err.to_string());
            error!("Failed to start http server: {}", err);
        }
    });

    // Spawn the QoS server
    spawn_server_task(async move {
        if let Err(err) = qos::start_qos_server().await {
            error_message("Failed to start qos server", &err.to_string());
            error!("Failed to start qos server: {}", err);
        }
    });

    // Spawn the telemetry server
    spawn_server_task(async move {
        if let Err(err) = telemetry::start_telemetry_server(http_client, base_url).await {
            error_message("Failed to start telemetry server", &err.to_string());
            error!("Failed to start telemetry server: {}", err);
        }
    });
}
