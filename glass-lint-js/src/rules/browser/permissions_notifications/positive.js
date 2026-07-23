// @case description positive fixture for browser:browser.permissions-notifications
// @tool glass-lint rules=browser:browser.permissions-notifications
// @expect-error glass-lint rule=browser:browser.permissions-notifications
Notification.requestPermission();
// The window-qualified spelling retains the configured browser root.
// @expect-error glass-lint rule=browser:browser.permissions-notifications
window.Notification.requestPermission();
// @expect-error glass-lint rule=browser:browser.permissions-notifications
globalThis.Notification.requestPermission();
// Aliases of the unshadowed static browser method are also detected.
const requestNotificationPermission = Notification.requestPermission;
// @expect-error glass-lint rule=browser:browser.permissions-notifications
requestNotificationPermission();
// @expect-error glass-lint rule=browser:browser.permissions-notifications
new Notification("ready");
// Service-worker registrations can display notifications without constructing
// a Window Notification object.
// @expect-error glass-lint rule=browser:browser.permissions-notifications
self.registration.showNotification("background");
