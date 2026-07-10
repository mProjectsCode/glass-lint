// @case description negative fixture for js:browser.permissions-notifications
// @tool glass-lint rules=js:browser.permissions-notifications
// @expect-no-error glass-lint rule=js:browser.permissions-notifications message_id=detected
// A local Notification class is not the browser API.
class Notification { static requestPermission() {} }
// @expect-no-error glass-lint rule=js:browser.permissions-notifications message_id=detected
Notification.requestPermission();

// Reassignment drops a previously rooted alias.
let requestNotificationPermission = globalThis.Notification.requestPermission;
requestNotificationPermission = () => {};
// @expect-no-error glass-lint rule=js:browser.permissions-notifications message_id=detected
requestNotificationPermission();
