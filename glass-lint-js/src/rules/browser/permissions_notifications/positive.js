// @case description positive fixture for browser:browser.permissions-notifications
// @tool glass-lint rules=browser:browser.permissions-notifications
// @expect-error glass-lint rule=browser:browser.permissions-notifications message_id=detected
Notification.requestPermission();
// The window-qualified spelling retains the configured browser root.
// @expect-error glass-lint rule=browser:browser.permissions-notifications message_id=detected
window.Notification.requestPermission();
// @expect-error glass-lint rule=browser:browser.permissions-notifications message_id=detected
globalThis.Notification.requestPermission();
// Aliases of the unshadowed static browser method are also detected.
const requestNotificationPermission = Notification.requestPermission;
// @expect-error glass-lint rule=browser:browser.permissions-notifications message_id=detected
requestNotificationPermission();
// @expect-error glass-lint rule=browser:browser.permissions-notifications message_id=detected
new Notification("ready");
// Service-worker registrations can display notifications without constructing
// a Window Notification object.
// @expect-error glass-lint rule=browser:browser.permissions-notifications message_id=detected
self.registration.showNotification("background");
