// @case description positive fixture for js:browser.permissions-notifications
// @tool glass-lint rules=js:browser.permissions-notifications
// @expect-error glass-lint rule=js:browser.permissions-notifications message_id=detected
Notification.requestPermission();
// Aliases of the unshadowed static browser method are also detected.
const requestNotificationPermission = Notification.requestPermission;
// @expect-error glass-lint rule=js:browser.permissions-notifications message_id=detected
requestNotificationPermission();
