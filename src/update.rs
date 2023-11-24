use std::{env::current_exe, process::exit};
use crate::constants::{APP_VERSION, GITHUB_REPOSITORY};
use log::{debug, error};
use native_windows_gui::{ simple_message, MessageParams, MessageIcons, MessageButtons, message, MessageChoice, error_message};
use pocket_relay_client_shared::{update::{get_latest_release, download_latest_release}, reqwest, Version};


/// Handles the updating process
pub async fn update(http_client: reqwest::Client) {
    let path = current_exe().expect("Unable to locate executable path");
    let parent = path.parent().expect("Missing exe parent directory");

    let asi_path = parent.join("asi");

    let old_file = asi_path.join("pocket-relay-plugin.asi");

    let tmp_file = asi_path.join("pocket-relay-plugin.asi.tmp-download");
    let tmp_old = asi_path.join("pocket-relay-plugin.asi.tmp-old");


      // Remove the old file if it exists
    if tmp_old.exists() {
        tokio::fs::remove_file(&tmp_old)
            .await
            .expect("Failed to remove old executable");
    }

        // Remove temp download file if it exists
    if tmp_file.exists() {
        tokio::fs::remove_file(&tmp_file)
            .await
            .expect("Failed to remove temp executable");
    }


    debug!("Checking for updates");
    let latest_release = match get_latest_release(&http_client, GITHUB_REPOSITORY).await {
        Ok(value) => value,
        Err(err) => {
            error!("Failed to fetch latest release: {}", err);
            return;
        }
    };

    let latest_tag = latest_release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&latest_release.tag_name);

    let latest_version = match Version::parse(latest_tag) {
        Ok(value) => value,
        Err(err) => {
            error!("Failed to parse version of latest release: {}", err);
            return;
        }
    };

    let current_version = Version::parse(APP_VERSION).expect("Failed to parse app version");

    if latest_version <= current_version {
        if current_version > latest_version {
            debug!("Future release is installed ({})", current_version);
        } else {
            debug!("Latest version is installed ({})", current_version);
        }

        return;
    }

    debug!("New version is available ({})", latest_version);

    let asset_name = "pocket-relay-plugin.asi";

    let asset = match latest_release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
    {
        Some(value) => value,
        None => {
            error!("Server release is missing the desired binary, cannot update");
            return;
        }
    };

    let msg = format!(
        "There is a new version of the client available, would you like to update automatically?\n\n\
        Your version: v{}\n\
        Latest Version: v{}\n",
        current_version, latest_version, 
    );


    let confirm = message( &MessageParams {
        title: "New version is available",
        content: &msg,
        buttons: MessageButtons::YesNo,
        icons: MessageIcons::Question
    });

    if !matches!(confirm, MessageChoice::Yes) {
        return;
    }
  
    debug!("Downloading release");

    match download_latest_release(&http_client, asset).await {
        Ok(bytes) => {
            // Save the downloaded file to the tmp path
            if let Err(err) = tokio::fs::write(&tmp_file, bytes).await {
                error_message("Failed to save downloaded update", &err.to_string());
                return;
            }
        }
        Err(err) => {
            error_message("Failed to download", &err.to_string());

            // Delete partially downloaded file if present
            if tmp_file.exists() {
                let _ = tokio::fs::remove_file(tmp_file).await;
            }

            return;
        }
    }

    debug!("Swapping executable files");

    tokio::fs::rename(&old_file, &tmp_old)
        .await
        .expect("Failed to rename executable to temp path");
    tokio::fs::rename(&tmp_file, old_file)
        .await
        .expect("Failed to rename executable");

    simple_message(
        "Update successfull",
        "The client has been updated, restart the game now to use the new version",
    );

    exit(0);
}
