use std::sync::Arc;

use crate::api::{try_lookup_host, LookupData, LookupError};
use log::{debug, error};
use std::future::Future;
use tokio::{join, sync::RwLock, task::JoinSet};

pub mod http;
pub mod main;
pub mod qos;
pub mod redirector;
pub mod telemetry;
pub mod packet;

/// Static variable used to store server tasks state
static SERVER_TASKS: RwLock<Option<JoinSet<()>>> = RwLock::const_new(None);

/// Attempts to connect to the provided target server.
/// If the connection succeeds then the local server
/// will start
///
/// # Arguments
/// * host - The host to attempt to connect to
pub async fn try_start_servers(host: String) -> Result<Arc<LookupData>, LookupError> {
    // Attempt to lookup the provided server
    let result = try_lookup_host(host).await?;
    let result = Arc::new(result);

    // Stop all existing server tasks
    stop_server_tasks().await;

    // Start new server tasks
    start_server_tasks(result.clone()).await;

    Ok(result)
}

/// Starts and waits for all the servers
async fn start_server_tasks(target: Arc<LookupData>) {
    // Write handle is obtained before starting the server
    // (Servers will depend on created task set so we cant let them read yet)
    let write = &mut *SERVER_TASKS.write().await;

    // Create the servers task set
    let task_set = write.insert(JoinSet::new());

    // Spawn the servers task
    task_set.spawn(async move {
        join!(
            main::start_server(target.clone()),
            qos::start_server(),
            redirector::start_server(),
            telemetry::start_server(target.clone()),
            http::start_server(target)
        );
    });
}

/// Stops all server related tasks (Disconnecting)
pub async fn stop_server_tasks() {
    if let Some(mut task) = SERVER_TASKS.write().await.take() {
        debug!("Stopping servers");
        task.abort_all();
    }
}

/// Blocking read to check if the servers are running
pub fn servers_running_blocking() -> bool {
    SERVER_TASKS.blocking_read().is_some()
}

/// Spawns an asyncronous task on the server tasks
/// queue so that it can be cancelled when the server
/// is stopped
///
/// # Arguments
/// * task - The task to spawn
pub async fn spawn_task<F>(task: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    let mut task_set = SERVER_TASKS.write().await;
    let Some(task_set) = &mut *task_set else {
        error!("Failed to spawn task, task set not initialized");
        return;
    };

    task_set.spawn(task);
}
