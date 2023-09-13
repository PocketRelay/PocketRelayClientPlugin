use crate::{
    constants::{MAIN_PORT, REDIRECTOR_PORT},
    servers::{packet::Packet, spawn_task},
};
use blaze_ssl_async::{BlazeAccept, BlazeListener};
use futures_util::{SinkExt, StreamExt};
use log::{debug, error};
use native_windows_gui::error_message;
use std::{io, net::Ipv4Addr, time::Duration};
use tdf::TdfSerialize;
use tokio::{select, time::sleep};
use tokio_util::codec::Framed;

use super::packet::PacketCodec;

/// Redirector server. Handles directing clients that connect to the local
/// proxy server that will connect them to the target server.
pub async fn start_server() {
    // Bind a listener for SSLv3 connections over TCP
    let listener = match BlazeListener::bind((Ipv4Addr::UNSPECIFIED, REDIRECTOR_PORT)).await {
        Ok(value) => value,
        Err(err) => {
            error_message("Failed to start redirector", &err.to_string());
            error!("Failed to start redirector: {}", err);
            return;
        }
    };

    // Accept incoming connections
    loop {
        // Accept a new connection
        let accept = match listener.accept().await {
            Ok(value) => value,
            Err(err) => {
                error!("Failed to accept redirector connection: {}", err);
                break;
            }
        };

        debug!("Redirector connection ->");

        // Spawn a handler for the listener
        spawn_task(async move {
            let _ = handle_client(accept).await;
        })
        .await;
    }
}

/// The timeout before idle redirector connections are terminated
/// (1 minutes before disconnect timeout)
static DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

const REDIRECTOR: u16 = 0x5;
const GET_SERVER_INSTANCE: u16 = 0x1;

/// Handles dealing with a redirector client
///
/// `stream`   The stream to the client
/// `addr`     The client address
/// `instance` The server instance information
async fn handle_client(accept: BlazeAccept) -> io::Result<()> {
    // Complete the SSLv3 handshaking process
    let (stream, _) = match accept.finish_accept().await {
        Ok(value) => value,
        Err(err) => {
            error!("Failed to accept redirector connection: {}", err);
            return Ok(());
        }
    };

    // Create a packet reader
    let mut framed = Framed::new(stream, PacketCodec);

    loop {
        let packet = select! {
            // Attempt to read packets from the stream
            result = framed.next() => result,
            // If the timeout completes before the redirect is complete the
            // request is considered over and terminates
            _ = sleep(DEFAULT_TIMEOUT) => { break; }
        };

        let packet = match packet.transpose()? {
            Some(value) => value,
            None => break,
        };

        let header = &packet.header;

        // Empty response for any unknown requests
        if header.component != REDIRECTOR || header.command != GET_SERVER_INSTANCE {
            // Empty response for packets that aren't asking to redirect
            framed.send(Packet::response_empty(&packet)).await?;
            continue;
        }

        debug!("Recieved instance request packet");

        // Response with the instance details
        let response = Packet::response(&packet, ServerInstanceResponse);
        framed.send(response).await?;
        break;
    }

    Ok(())
}

/// Packet contents for providing the redirection details
/// for 127.0.0.1 to allow proxying
pub struct ServerInstanceResponse;

impl TdfSerialize for ServerInstanceResponse {
    fn serialize<S: tdf::TdfSerializer>(&self, w: &mut S) {
        // Local server address
        w.tag_union_start(b"ADDR", 0x0);
        w.group(b"VALU", |w| {
            w.tag_owned(b"IP", u32::from_be_bytes([127, 0, 0, 1]));
            w.tag_owned(b"PORT", MAIN_PORT);
        });

        // Disable SSLv3 use raw TCP
        w.tag_bool(b"SECU", false);
        w.tag_bool(b"XDNS", false);
    }
}
