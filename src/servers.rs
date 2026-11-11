use crate::{
    core::{ctx::ClientContext, servers::*},
    tunnel_state::{TunnelState, TunnelStateRef},
    ui::error_message,
};
use log::error;
use pocket_relay_client_shared::events::TunnelEvent;
use std::{future::Future, sync::Arc};
use tokio::sync::mpsc;

/// Starts all the servers in their own tasks
///
/// ## Arguments
/// * `ctx`           - The client context
/// * `tunnel_state_ref` - Reference to update the tunnel state
pub fn start_all_servers(ctx: Arc<ClientContext>, tunnel_state_ref: TunnelStateRef) {
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
        let tunnel = start_tunnel_server(ctx.clone(), tunnel_state_ref);
        run_server(tunnel, "tunnel");
    } else {
        tunnel_state_ref.set_state(TunnelState::Unavailable);
    }

    // Spawn telemetry server
    let telemetry = telemetry::start_telemetry_server(ctx);
    run_server(telemetry, "telemetry");
}

/// Background worker to process tunnel events and update the
/// tunnel state accordingly
fn tunnel_state_worker(
    tunnel_state_ref: TunnelStateRef,
    mut event_rx: mpsc::UnboundedReceiver<TunnelEvent>,
) {
    spawn_server_task(async move {
        while let Some(event) = event_rx.recv().await {
            tunnel_state_ref.set_state(TunnelState::from(event));
        }
    });
}

/// Runs the tunnel server, if a tunnel port is available a UDP tunnel will be
/// attempted, if that fails or a tunnel port is unavailable an HTTP tunnel
/// will be attempted instead
async fn start_tunnel_server(
    ctx: Arc<ClientContext>,
    tunnel_state_ref: TunnelStateRef,
) -> std::io::Result<()> {
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    tunnel_state_worker(tunnel_state_ref, event_rx);

    // Spawn tunnel server
    match ctx.tunnel_port {
        // When UDP tunnel server port is available use the faster UDP tunnel server
        Some(tunnel_port) => {
            let err = match udp_tunnel::start_udp_tunnel_server(
                ctx.clone(),
                tunnel_port,
                event_tx.clone(),
            )
            .await
            {
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
            tunnel::start_tunnel_server(ctx, event_tx).await
        }
        // When unavailable fallback to the HTTP upgrade tunnel
        None => tunnel::start_tunnel_server(ctx, event_tx).await,
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
