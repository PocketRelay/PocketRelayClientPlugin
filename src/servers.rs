use crate::{
    core::{ctx::ClientContext, servers::*},
    ui::error_message,
};
use log::error;
use native_windows_gui::NoticeSender;
use std::{
    future::Future,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
};

/// State of the tunnel server task
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelState {
    /// Server doesn't offer tunneling (or no servers are running)
    Off,
    /// Tunnel task is running
    Active,
    /// Tunnel task has stopped (gave up reconnecting)
    Failed,
}

static TUNNEL_STATE: AtomicU8 = AtomicU8::new(0);

fn set_tunnel_state(state: TunnelState) {
    TUNNEL_STATE.store(state as u8, Ordering::SeqCst);
}

/// Gets the current state of the tunnel server task
pub fn tunnel_state() -> TunnelState {
    match TUNNEL_STATE.load(Ordering::SeqCst) {
        1 => TunnelState::Active,
        2 => TunnelState::Failed,
        _ => TunnelState::Off,
    }
}

/// Resets the tunnel state (used when disconnecting)
pub fn reset_tunnel_state() {
    set_tunnel_state(TunnelState::Off);
}

/// Starts all the servers in their own tasks
///
/// ## Arguments
/// * `ctx`           - The client context
/// * `tunnel_notice` - Sender used to wake the UI when the tunnel state changes
pub fn start_all_servers(ctx: Arc<ClientContext>, tunnel_notice: NoticeSender) {
    // Stop existing servers and tasks if they are running
    stop_server_tasks();

    // Spawn redirector server
    let redirector = redirector::start_redirector_server();
    run_server(redirector, "redirector");

    // Spawn blaze server
    let blaze = blaze::start_blaze_server(ctx.clone());
    run_server(blaze, "blaze");

    // Spawn http proxy server
    let http = http::start_http_server(ctx.clone());
    run_server(http, "http");

    // Spawn QoS server
    let qos = qos::start_qos_server();
    run_server(qos, "qos");

    // Spawn tunnel server
    // The tunnel is only used when the server provides a UDP tunnel port or
    // an association token (required by the HTTP tunnel)
    if ctx.tunnel_port.is_some() || ctx.association.is_some() {
        set_tunnel_state(TunnelState::Active);
        let tunnel = start_tunnel_server(ctx.clone());
        spawn_server_task(async move {
            let result = tunnel.await;
            set_tunnel_state(TunnelState::Failed);
            tunnel_notice.notice();
            if let Err(err) = result {
                error_message("Failed to start tunnel server", &err.to_string());
                error!("Failed to start tunnel server: {err}");
            }
        });
    } else {
        set_tunnel_state(TunnelState::Off);
    }

    // Spawn telemetry server
    let telemetry = telemetry::start_telemetry_server(ctx);
    run_server(telemetry, "telemetry");
}

/// Runs the tunnel server, if a tunnel port is available a UDP tunnel will be
/// attempted, if that fails or a tunnel port is unavailable an HTTP tunnel
/// will be attempted instead
async fn start_tunnel_server(ctx: Arc<ClientContext>) -> std::io::Result<()> {
    // Spawn tunnel server
    match ctx.tunnel_port {
        // When UDP tunnel server port is available use the faster UDP tunnel server
        Some(tunnel_port) => {
            let err = match udp_tunnel::start_udp_tunnel_server(ctx.clone(), tunnel_port).await {
                // Encountered error with UDP tunnel
                Err(err) => err,
                // Server exited normally
                Ok(_) => return Ok(()),
            };

            error!(
                "error using UDP tunnel, falling back to HTTP tunnel: {}",
                err
            );

            // Error while connecting UDP tunnel, fallback to HTTP upgrade tunnel
            tunnel::start_tunnel_server(ctx).await
        }
        // When unavailable fallback to the HTTP upgrade tunnel
        None => tunnel::start_tunnel_server(ctx).await,
    }
}

/// Runs the provided server `future` in a background task displaying
/// and logging any errors if they occur
#[inline]
pub fn run_server<F>(future: F, name: &'static str)
where
    F: Future<Output = std::io::Result<()>> + Send + 'static,
{
    spawn_server_task(async move {
        if let Err(err) = future.await {
            error_message(&format!("Failed to start {name} server"), &err.to_string());
            error!("Failed to start {name} server: {err}");
        }
    });
}
