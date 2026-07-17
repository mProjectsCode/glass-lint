// @case description positive fixture for browser:browser.permissions-media
// @tool glass-lint rules=browser:browser.permissions-media
// Direct browser media requests are detected regardless of constraints.
// @expect-error glass-lint rule=browser:browser.permissions-media message_id=detected
navigator.mediaDevices.getUserMedia({ audio: true });
const media = navigator.mediaDevices;
// Derived aliases retain browser provenance.
// @expect-error glass-lint rule=browser:browser.permissions-media message_id=detected
media.getUserMedia({ video: true });
