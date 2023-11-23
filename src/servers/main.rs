use crate::{
    api::LookupData,
    constants::{APP_VERSION, HTTP_PORT, MAIN_PORT},
    servers::spawn_task,
};
use hyper::header::USER_AGENT;
use log::{debug, error};
use native_windows_gui::error_message;
use reqwest::{
    header::{self, HeaderMap, HeaderValue},
    Client,
};
use std::{net::Ipv4Addr, sync::Arc};
use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream},
};

/// Starts the main server proxy. This creates a connection to the Pocket Relay
/// which is upgraded and then used as the main connection fro the game.
pub async fn start_server(target: Arc<LookupData>) {
    // Initializing the underlying TCP listener
    let listener = match TcpListener::bind((Ipv4Addr::UNSPECIFIED, MAIN_PORT)).await {
        Ok(value) => value,
        Err(err) => {
            error_message("Failed to start main", &err.to_string());
            error!("Failed to start main: {}", err);
            return;
        }
    };

    // Accept incoming connections
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(value) => value,
            Err(err) => {
                error!("Failed to accept main connection: {}", err);
                break;
            }
        };

        debug!("Main connection ->");

        // Spawn off a new handler for the connection
        spawn_task(handle_blaze(stream, target.clone())).await;
    }
}

/// Header for the Pocket Relay connection scheme used by the client
const LEGACY_HEADER_SCHEME: &str = "X-Pocket-Relay-Scheme";
/// Header for the Pocket Relay connection port used by the client
const LEGACY_HEADER_PORT: &str = "X-Pocket-Relay-Port";
/// Header for the Pocket Relay connection host used by the client
const LEGACY_HEADER_HOST: &str = "X-Pocket-Relay-Host";
/// Header to tell the server to use local HTTP
const HEADER_LOCAL_HTTP: &str = "X-Pocket-Relay-Local-Http";
/// Endpoint for upgrading the server connection
const UPGRADE_ENDPOINT: &str = "api/server/upgrade";

async fn handle_blaze(mut client: TcpStream, target: Arc<LookupData>) {
    // Create the upgrade URL
    let url = target
        .url
        .join(UPGRADE_ENDPOINT)
        .expect("Failed to create upgrade endpoint URL");

    let user_agent = format!("PocketRelayClient/v{}", APP_VERSION);

    // Create the HTTP Upgrade headers
    let mut headers = HeaderMap::new();
    headers.insert(header::CONNECTION, HeaderValue::from_static("Upgrade"));
    headers.insert(header::UPGRADE, HeaderValue::from_static("blaze"));
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(&user_agent).expect("User agent header was invalid"),
    );

    // Append use local http header
    headers.insert(HEADER_LOCAL_HTTP, HeaderValue::from_static("true"));

    // Append legacy http details headers
    headers.insert(LEGACY_HEADER_SCHEME, HeaderValue::from_static("http"));
    headers.insert(LEGACY_HEADER_PORT, HeaderValue::from(HTTP_PORT));
    headers.insert(LEGACY_HEADER_HOST, HeaderValue::from_static("127.0.0.1"));

    debug!("Connecting pipe to Pocket Relay server");

    // Create the request
    let request = Client::new().get(url).headers(headers).send();

    // Await the server response to the request
    let response = match request.await {
        Ok(value) => value,
        Err(err) => {
            error!("Failed to get server pipe response: {}", err);
            return;
        }
    };

    // Check the server response wasn't an error
    let response = match response.error_for_status() {
        Ok(value) => value,
        Err(err) => {
            error!("Server upgrade responded with error: {}", err);
            return;
        }
    };

    // Server connection gained through upgrading the client
    let mut server = match response.upgrade().await {
        Ok(value) => value,
        Err(err) => {
            error!("Failed to upgrade connection pipe: {}", err);
            return;
        }
    };

    // Copy the data between the connection
    let _ = copy_bidirectional(&mut client, &mut server).await;
}
