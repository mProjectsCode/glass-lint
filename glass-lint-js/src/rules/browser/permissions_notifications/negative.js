// @case description negative fixture for browser:browser.permissions-notifications
// @tool glass-lint rules=browser:browser.permissions-notifications
// @expect-no-error glass-lint rule=browser:browser.permissions-notifications
// A local Notification class is not the browser API.
class Notification { static requestPermission() {} }
// @expect-no-error glass-lint rule=browser:browser.permissions-notifications
Notification.requestPermission();
// @expect-no-error glass-lint rule=browser:browser.permissions-notifications
new Notification("local");

// Reassignment drops a previously rooted alias.
let requestNotificationPermission = globalThis.Notification.requestPermission;
requestNotificationPermission = () => {};
// @expect-no-error glass-lint rule=browser:browser.permissions-notifications
requestNotificationPermission();

// A local self/registration-shaped object is not the service-worker global.
function localWorker(self) {
    // @expect-no-error glass-lint rule=browser:browser.permissions-notifications
    self.registration.showNotification("local");
}
localWorker({ registration: { showNotification() {} } });

function localWindow(window) {
    // @expect-no-error glass-lint rule=browser:browser.permissions-notifications
    window.Notification.requestPermission();
}
localWindow({ Notification: { requestPermission() {} } });
