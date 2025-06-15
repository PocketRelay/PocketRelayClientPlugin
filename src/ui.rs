use native_windows_gui::{message, MessageButtons, MessageChoice, MessageIcons, MessageParams};

/// Shows a confirmation message to the user returning
/// the choice that the user made.
///
/// ## Arguments
/// * `title` - The title for the dialog
/// * `text`  - The text for the dialog
pub fn confirm_message(title: &str, text: &str) -> bool {
    let choice = message(&MessageParams {
        title,
        content: text,
        buttons: MessageButtons::YesNo,
        icons: MessageIcons::Question,
    });

    matches!(choice, MessageChoice::Yes)
}

/// Shows a info message to the user.
///
/// ## Arguments
/// * `title` - The title for the dialog
/// * `text`  - The text for the dialog
pub fn info_message(title: &str, text: &str) {
    message(&MessageParams {
        title,
        content: text,
        buttons: MessageButtons::Ok,
        icons: MessageIcons::Info,
    });
}

/// Shows an error message to the user.
///
/// ## Arguments
/// * `title` - The title for the dialog
/// * `text`  - The text for the dialog
pub fn error_message(title: &str, text: &str) {
    message(&MessageParams {
        title,
        content: text,
        buttons: MessageButtons::Ok,
        icons: MessageIcons::Error,
    });
}
