// @case description positive fixture for browser:browser.permissions-notifications
// @tool glass-lint rules=browser:browser.permissions-notifications
// @expect-error glass-lint rule=browser:browser.permissions-notifications message_id=detected
Notification.requestPermission();
// Aliases of the unshadowed static browser method are also detected.
const requestNotificationPermission = Notification.requestPermission;
// @expect-error glass-lint rule=browser:browser.permissions-notifications message_id=detected
requestNotificationPermission();
