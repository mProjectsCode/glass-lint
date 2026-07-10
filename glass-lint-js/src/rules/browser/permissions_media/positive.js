// @case description positive fixture for js:browser.permissions-media
// @tool glass-lint rules=js:browser.permissions-media
// @expect-error glass-lint rule=js:browser.permissions-media message_id=detected
navigator.mediaDevices.getUserMedia({audio:true});
// second independent example
// @expect-error glass-lint rule=js:browser.permissions-media message_id=detected
navigator.mediaDevices.getUserMedia({ video: true });
