// @case description positive fixture for browser:browser.environment
// @tool glass-lint rules=browser:browser.environment
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
navigator.userAgent;
// Window and worker-qualified navigator paths retain browser identity.
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
window.navigator.userAgent;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
window.navigator.connection.effectiveType;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
self.navigator.language;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
self.navigator.hardwareConcurrency;
// Every configured property is a direct-read heuristic.
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
navigator.language;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
screen.width;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
screen.availWidth;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
screen.availHeight;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
screen.colorDepth;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
screen.pixelDepth;
// Rooted window.screen reads preserve the receiver identity.
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
window.screen.width;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
window.screen.availWidth;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
window.screen.colorDepth;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
navigator.languages;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
navigator.hardwareConcurrency;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
navigator.deviceMemory;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
navigator.vendor;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
navigator.cookieEnabled;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
navigator.maxTouchPoints;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
navigator.doNotTrack;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
navigator.webdriver;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
navigator.pdfViewerEnabled;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
navigator.onLine;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
navigator.connection.effectiveType;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
navigator.connection.rtt;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
navigator.connection.downlink;
// @expect-error glass-lint rule=browser:browser.environment message_id=detected
navigator.connection.saveData;

// Rooted navigator reads reject shadowed local lookalikes.
function inspect(navigator) {
    // @expect-no-error glass-lint rule=browser:browser.environment message_id=detected
    navigator.userAgent;
}
