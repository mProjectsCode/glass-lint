// @case description shadowed, dynamic, reassigned, and unsupported permission queries
// @tool glass-lint rules=browser:browser.permissions-query
const navigator = { permissions: { query() {} } };
// @expect-no-error glass-lint rule=browser:browser.permissions-query message_id=detected
navigator.permissions.query({ name: "local" });

const property = getPropertyName();
// @expect-no-error glass-lint rule=browser:browser.permissions-query message_id=detected
globalThis.navigator.permissions[property]({});

let permissions = globalThis.navigator.permissions;
permissions = localPermissions;
// @expect-no-error glass-lint rule=browser:browser.permissions-query message_id=detected
permissions.query({ name: "reassigned" });

function localWindow(window) {
    // @expect-no-error glass-lint rule=browser:browser.permissions-query message_id=detected
    window.navigator.permissions.query({ name: "local" });
}
localWindow({ navigator: { permissions: { query() {} } } });
