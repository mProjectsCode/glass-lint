// @case description positive fixture for js:browser.permissions-notifications
// @tool glass-lint rules=js:browser.permissions-notifications
// @expect-error glass-lint rule=js:browser.permissions-notifications message_id=detected
Notification.requestPermission();
// second independent example
// @expect-error glass-lint rule=js:browser.permissions-notifications message_id=detected
Notification.requestPermission();
