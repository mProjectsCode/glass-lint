// @case description positive fixture for browser:browser.permissions-media
// @tool glass-lint rules=browser:browser.permissions-media
// Direct browser media requests are detected regardless of constraints.
// @expect-error glass-lint rule=browser:browser.permissions-media message_id=detected
navigator.mediaDevices.getUserMedia({ audio: true });
// @expect-error glass-lint rule=browser:browser.permissions-media message_id=detected
window.navigator.mediaDevices.getUserMedia({ video: true });
// @expect-error glass-lint rule=browser:browser.permissions-media message_id=detected
self.navigator.mediaDevices.enumerateDevices();
// @expect-error glass-lint rule=browser:browser.permissions-media message_id=detected
globalThis.navigator.mediaDevices.getUserMedia({ audio: true });
const media = navigator.mediaDevices;
// Derived aliases retain browser provenance.
// @expect-error glass-lint rule=browser:browser.permissions-media message_id=detected
media.getUserMedia({ video: true });
// @expect-error glass-lint rule=browser:browser.permissions-media message_id=detected
navigator.mediaDevices.getDisplayMedia({ video: true });
// @expect-error glass-lint rule=browser:browser.permissions-media message_id=detected
navigator.mediaDevices.enumerateDevices();
