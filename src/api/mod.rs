use std::str::FromStr;

use hyper::{
    header::{ACCEPT, USER_AGENT},
    StatusCode,
};
use log::error;
use reqwest::Client;
use semver::Version;
use serde::Deserialize;
use thiserror::Error;
use url::Url;

use crate::constants::{APP_VERSION, MIN_SERVER_VERSION, SERVER_IDENT};

/// Details provided by the server. These are the only fields
/// that we need the rest are ignored by this client.
#[derive(Deserialize)]
struct ServerDetails {
    /// The Pocket Relay version of the server
    version: Version,
    /// Server identifier checked to ensure its a proper server
    #[serde(default)]
    ident: Option<String>,
}

/// Data from completing a lookup contains the resolved address
/// from the connection to the server as well as the server
/// version obtained from the server
#[derive(Debug, Clone)]
pub struct LookupData {
    /// The server url
    pub url: Url,
    /// The server version
    pub version: Version,
}

/// Errors that can occur while looking up a server
#[derive(Debug, Error)]
pub enum LookupError {
    /// The server url was invalid
    #[error("Invalid Connection URL: {0}")]
    InvalidHostTarget(#[from] url::ParseError),
    /// The server connection failed
    #[error("Failed to connect to server: {0}")]
    ConnectionFailed(reqwest::Error),
    /// The server gave an invalid response likely not a PR server
    #[error("Server replied with error response: {0} {1}")]
    ErrorResponse(StatusCode, reqwest::Error),
    /// The server gave an invalid response likely not a PR server
    #[error("Invalid server response: {0}")]
    InvalidResponse(reqwest::Error),
    /// Server wasn't a valid pocket relay server
    #[error("Server identifier was incorrect (Not a PocketRelay server?)")]
    NotPocketRelay,
    /// Server version is too old
    #[error("Server version is too outdated ({0}) this client requires servers of version {1} or greater")]
    ServerOutdated(Version, Version),
}

/// Attempts to connect to the Pocket Relay HTTP server at the provided
/// host. Will make a connection to the /api/server endpoint and if the
/// response is a valid ServerDetails message then the server is
/// considered valid.
///
/// `host` The host to try and lookup
pub async fn try_lookup_host(host: &str) -> Result<LookupData, LookupError> {
    let url = {
        let mut url = String::new();

        // Fill in missing scheme portion
        if !host.starts_with("http://") && !host.starts_with("https://") {
            url.push_str("http://");
            url.push_str(host)
        } else {
            url.push_str(host);
        }

        // Ensure theres a trailing slash (URL path will be interpeted incorrectly without)
        if !host.ends_with('/') {
            url.push('/');
        }

        url
    };

    let url = Url::from_str(&url)?;
    let info_url = url.join("api/server").expect("Failed to server info URL");

    let client = Client::new();

    let response = client
        .get(info_url)
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, format!("PocketRelayClient/v{}", APP_VERSION))
        .send()
        .await
        .map_err(LookupError::ConnectionFailed)?;

    #[cfg(debug_assertions)]
    {
        use log::debug;

        debug!("Response Status: {}", response.status());
        debug!("HTTP Version: {:?}", response.version());
        debug!("Content Length: {:?}", response.content_length());
        debug!("HTTP Headers: {:?}", response.headers());
    }

    let response = match response.error_for_status() {
        Ok(value) => value,
        Err(err) => {
            error!("Server responded with error: {}", err);
            return Err(LookupError::ErrorResponse(
                err.status().unwrap_or_default(),
                err,
            ));
        }
    };

    let details = response
        .json::<ServerDetails>()
        .await
        .map_err(LookupError::InvalidResponse)?;

    // Handle invalid server ident
    if details.ident.is_none() || details.ident.is_some_and(|value| value != SERVER_IDENT) {
        return Err(LookupError::NotPocketRelay);
    }

    if details.version < MIN_SERVER_VERSION {
        return Err(LookupError::ServerOutdated(
            details.version,
            MIN_SERVER_VERSION,
        ));
    }

    Ok(LookupData {
        url,
        version: details.version,
    })
}
