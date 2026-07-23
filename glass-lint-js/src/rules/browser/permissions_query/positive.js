// @case description rooted browser permission queries and aliases
// @tool glass-lint rules=browser:browser.permissions-query
// @expect-error glass-lint rule=browser:browser.permissions-query
navigator.permissions.query({ name: "geolocation" });
// @expect-error glass-lint rule=browser:browser.permissions-query
window.navigator.permissions.query({ name: "camera" });
// @expect-error glass-lint rule=browser:browser.permissions-query
self.navigator.permissions.query({ name: "notifications" });
// @expect-error glass-lint rule=browser:browser.permissions-query
globalThis.navigator.permissions.query({ name: "camera" });
const permissions = navigator.permissions;
// @expect-error glass-lint rule=browser:browser.permissions-query
permissions.query({ name: "notifications" });
// @expect-error glass-lint rule=browser:browser.permissions-query
navigator.permissions["query"]({ name: "camera" });
