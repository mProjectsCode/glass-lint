// @case description rooted browser permission queries and aliases
// @tool glass-lint rules=browser:browser.permissions-query
// @expect-error glass-lint rule=browser:browser.permissions-query message_id=detected
navigator.permissions.query({ name: "geolocation" });
// @expect-error glass-lint rule=browser:browser.permissions-query message_id=detected
window.navigator.permissions.query({ name: "camera" });
// @expect-error glass-lint rule=browser:browser.permissions-query message_id=detected
self.navigator.permissions.query({ name: "notifications" });
// @expect-error glass-lint rule=browser:browser.permissions-query message_id=detected
globalThis.navigator.permissions.query({ name: "camera" });
const permissions = navigator.permissions;
// @expect-error glass-lint rule=browser:browser.permissions-query message_id=detected
permissions.query({ name: "notifications" });
// @expect-error glass-lint rule=browser:browser.permissions-query message_id=detected
navigator.permissions["query"]({ name: "camera" });
