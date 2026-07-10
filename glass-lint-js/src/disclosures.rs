pub(crate) fn for_rule(id: &str) -> &'static [&'static str] {
    match id {
        "network.request" | "node.network" | "dom.remote-resource" => {
            &["disclosure.network_access"]
        }
        "node.filesystem" => &["disclosure.node_filesystem_access"],
        "node.subprocess" | "electron.ipc" | "electron.shell" => {
            &["disclosure.process_or_shell_access"]
        }
        "browser.clipboard-read" | "browser.clipboard-write" => &["disclosure.clipboard_access"],
        "browser.permissions-geolocation"
        | "browser.permissions-media"
        | "browser.permissions-bluetooth"
        | "browser.permissions-notifications" => &["disclosure.permission_sensitive_browser_api"],
        "dynamic-code.eval" | "dynamic-code.string-timer" | "dynamic-code.script-injection" => {
            &["disclosure.dynamic_code_or_remote_code"]
        }
        _ => &[],
    }
}
