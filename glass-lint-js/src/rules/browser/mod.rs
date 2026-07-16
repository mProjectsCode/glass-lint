mod clipboard_read;
mod clipboard_write;
mod environment;
mod file_dialog;
mod global_input_hook;
mod permissions_bluetooth;
mod permissions_geolocation;
mod permissions_media;
mod permissions_notifications;
mod persistent_storage;
mod remote_resource;
mod request;
mod script_injection;

use glass_lint_core::rules::Rule;

pub fn rules() -> Vec<Rule> {
    vec![
        clipboard_read::rule(),
        clipboard_write::rule(),
        persistent_storage::rule(),
        permissions_geolocation::rule(),
        permissions_media::rule(),
        permissions_bluetooth::rule(),
        permissions_notifications::rule(),
        environment::rule(),
        global_input_hook::rule(),
        file_dialog::rule(),
        request::rule(),
        remote_resource::rule(),
        script_injection::rule(),
    ]
}
