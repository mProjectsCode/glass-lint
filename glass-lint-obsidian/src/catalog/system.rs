use super::ObsidianRuleBuilderExt;

use glass_lint_core::rules::{
    CallMatcher, Confidence, FlowMatcher, FlowValueMatcher, Matcher, MemberCallMatcher, Rule,
    Rule as ApiRule, Severity as ApiSeverity,
};

pub(super) fn rules() -> Vec<Rule> {
    vec![
        ApiRule::builder("plugins.internal_access")
            .label("Accesses plugin internals or other plugins")
            .category("dependency")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .with_rooted_member_reads([
                "app.plugins",
                "app.plugins.enabledPlugins",
                "app.plugins.manifests",
            ])
            .with_rooted_member_calls(["app.plugins.getPlugin"])
            .implies(["disclosure.plugin_internals"])
            .build(),
        ApiRule::builder("platform.branching")
            .label("Checks desktop, mobile, or OS platform flags")
            .category("dependency")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .with_module_member_calls(
                "obsidian",
                [
                    "Platform.isMobile",
                    "Platform.isDesktop",
                    "Platform.isMobileApp",
                    "Platform.isDesktopApp",
                    "Platform.isIosApp",
                    "Platform.isAndroidApp",
                    "Platform.isPhone",
                    "Platform.isTablet",
                    "Platform.isMacOS",
                    "Platform.isWin",
                    "Platform.isLinux",
                    "Platform.isSafari",
                ],
            )
            .with_module_member_reads(
                "obsidian",
                [
                    "Platform.isMobile",
                    "Platform.isDesktop",
                    "Platform.isMobileApp",
                    "Platform.isDesktopApp",
                    "Platform.isIosApp",
                    "Platform.isAndroidApp",
                    "Platform.isPhone",
                    "Platform.isTablet",
                    "Platform.isMacOS",
                    "Platform.isWin",
                    "Platform.isLinux",
                    "Platform.isSafari",
                ],
            )
            .with_heuristic_member_reads([
                "Platform.isMobile",
                "Platform.isDesktop",
                "Platform.isMobileApp",
                "Platform.isDesktopApp",
                "Platform.isIosApp",
                "Platform.isAndroidApp",
                "Platform.isPhone",
                "Platform.isTablet",
                "Platform.isMacOS",
                "Platform.isWin",
                "Platform.isLinux",
                "Platform.isSafari",
            ])
            .implies(["disclosure.platform_branching"])
            .build(),
        ApiRule::builder("filesystem.node")
            .label("Uses direct Node filesystem-related modules")
            .category("filesystem")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .with_imports([
                "fs",
                "fs/promises",
                "node:fs",
                "node:fs/promises",
                "path",
                "node:path",
                "os",
                "node:os",
                "stream",
                "node:stream",
                "buffer",
                "node:buffer",
                "zlib",
                "node:zlib",
            ])
            .implies(["disclosure.node_filesystem_access"])
            .build(),
        ApiRule::builder("process.node")
            .label("Uses Node process or subprocess APIs")
            .category("filesystem")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .with_imports(["child_process", "node:child_process"])
            .with_rooted_member_reads(["process.env", "process.platform"])
            .implies(["disclosure.process_or_shell_access"])
            .build(),
        ApiRule::builder("electron.desktop")
            .label("Uses Electron desktop APIs")
            .category("electron")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .with_imports(["electron"])
            .with_heuristic_member_calls([
                "shell.openExternal",
                "ipcRenderer.send",
                "ipcRenderer.invoke",
            ])
            .build(),
        ApiRule::builder("electron.ipc_shell")
            .label("Uses Electron IPC or shell APIs")
            .category("electron")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .with_heuristic_member_calls([
                "shell.openExternal",
                "shell.openPath",
                "ipcRenderer.send",
                "ipcRenderer.invoke",
                "remote.require",
            ])
            .implies(["disclosure.process_or_shell_access"])
            .build(),
        ApiRule::builder("browser.clipboard")
            .label("Reads or writes clipboard data")
            .category("browser")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .with_rooted_member_calls([
                "navigator.clipboard.read",
                "navigator.clipboard.readText",
                "navigator.clipboard.write",
                "navigator.clipboard.writeText",
            ])
            .implies(["disclosure.clipboard_access"])
            .build(),
        ApiRule::builder("browser.storage")
            .label("Uses persistent browser storage")
            .category("browser")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::High)
            .with_heuristic_member_reads(["localStorage", "sessionStorage", "indexedDB", "caches"])
            .with_heuristic_member_calls([
                "localStorage.getItem",
                "localStorage.setItem",
                "sessionStorage.getItem",
                "sessionStorage.setItem",
                "indexedDB.open",
                "caches.open",
            ])
            .build(),
        ApiRule::builder("browser.permissions")
            .label("Uses permission-sensitive browser APIs")
            .category("browser")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .with_rooted_member_calls([
                "navigator.geolocation.getCurrentPosition",
                "navigator.mediaDevices.getUserMedia",
                "Notification.requestPermission",
                "navigator.bluetooth.requestDevice",
            ])
            .implies(["disclosure.permission_sensitive_browser_api"])
            .build(),
        ApiRule::builder("browser.permission_availability")
            .label("Checks permission-sensitive browser API availability")
            .category("browser")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .with_heuristic_member_reads([
                "navigator.geolocation",
                "navigator.mediaDevices",
                "Notification",
                "RTCPeerConnection",
                "navigator.bluetooth",
            ])
            .build(),
        ApiRule::builder("browser.environment")
            .label("Reads browser or device environment data")
            .category("browser")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .with_heuristic_member_reads([
                "navigator.userAgent",
                "navigator.platform",
                "navigator.language",
                "Intl.DateTimeFormat",
                "screen.width",
                "screen.height",
            ])
            .implies(["disclosure.browser_environment_access"])
            .build(),
        ApiRule::builder("browser.broad_input_hooks")
            .label("Registers broad keyboard handlers or clipboard hooks")
            .category("browser")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .matcher(Matcher::member_call(
                MemberCallMatcher::syntactic_heuristic("document.addEventListener")
                    .arg_string(0, ["keydown", "keyup", "paste", "copy", "cut"]),
            ))
            .matcher(Matcher::member_call(
                MemberCallMatcher::syntactic_heuristic("window.addEventListener")
                    .arg_string(0, ["keydown", "keyup", "paste", "copy", "cut"]),
            ))
            .with_heuristic_member_calls([
                "navigator.clipboard.readText",
                "navigator.clipboard.writeText",
            ])
            .implies(["disclosure.global_handlers_or_timers"])
            .build(),
        ApiRule::builder("archive.compression")
            .label("Uses compression or archive handling")
            .category("filesystem")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .with_imports(["jszip", "tar", "zlib", "node:zlib", "fflate"])
            .with_string_literals(["gzip", "zip"])
            .build(),
        ApiRule::builder("crypto.hashing")
            .label("Uses cryptography or hashing APIs")
            .category("filesystem")
            .severity(ApiSeverity::Info)
            .confidence(Confidence::Medium)
            .with_imports(["crypto", "node:crypto", "crypto-js"])
            .with_heuristic_member_calls([
                "crypto.subtle.digest",
                "crypto.subtle.encrypt",
                "crypto.subtle.decrypt",
            ])
            .with_string_literals(["sha256", "SHA-256", "AES-GCM"])
            .build(),
        ApiRule::builder("dynamic_code")
            .label("Evaluates dynamic code or injects scripts")
            .category("dynamic_code")
            .severity(ApiSeverity::Warning)
            .confidence(Confidence::High)
            .with_heuristic_calls(["import"])
            .with_global_calls(["eval"])
            .with_global_calls(["Function"])
            .with_heuristic_constructors(["Function"])
            .matcher(Matcher::member_call(
                MemberCallMatcher::syntactic_heuristic("eval.call").static_string_arg(1),
            ))
            .matcher(Matcher::member_call(
                MemberCallMatcher::rooted_chain("globalThis.eval").static_string_arg(0),
            ))
            .matcher(Matcher::member_call(
                MemberCallMatcher::rooted_chain("window.eval").static_string_arg(0),
            ))
            .matcher(Matcher::call(
                CallMatcher::global("setTimeout").static_string_arg(0),
            ))
            .matcher(Matcher::call(
                CallMatcher::global("setInterval").static_string_arg(0),
            ))
            .matcher(Matcher::member_call(
                MemberCallMatcher::rooted_chain("globalThis.setTimeout").static_string_arg(0),
            ))
            .matcher(Matcher::member_call(
                MemberCallMatcher::rooted_chain("globalThis.setInterval").static_string_arg(0),
            ))
            .matcher(Matcher::member_call(
                MemberCallMatcher::rooted_chain("window.setTimeout").static_string_arg(0),
            ))
            .matcher(Matcher::member_call(
                MemberCallMatcher::rooted_chain("window.setInterval").static_string_arg(0),
            ))
            .matcher(Matcher::flow(
                FlowMatcher::new("script insertion".to_string())
                    .source_member_call("document.createElement")
                    .source_arg_string(0, ["script"])
                    .property_write("src", FlowValueMatcher::Any)
                    .property_write("textContent", FlowValueMatcher::Any)
                    .member_call_config(
                        "setAttribute",
                        [
                            (0, FlowValueMatcher::StaticExact(vec!["src".to_string()])),
                            (1, FlowValueMatcher::Any),
                        ],
                    )
                    .member_call_config("append", [])
                    .sink_member_call_arg_indices(
                        [
                            "document.head.appendChild",
                            "document.body.appendChild",
                            "document.documentElement.appendChild",
                            "document.documentElement.insertBefore",
                        ],
                        [0],
                    )
                    .sink_member_call_any_arg([
                        "document.head.append",
                        "document.body.append",
                        "document.body.prepend",
                        "document.documentElement.append",
                        "document.documentElement.prepend",
                    ]),
            ))
            .implies(["disclosure.dynamic_code_or_remote_code"])
            .build(),
    ]
    .into_iter()
    .map(|rule| rule.expect("built-in Obsidian rule should be valid"))
    .collect()
}
