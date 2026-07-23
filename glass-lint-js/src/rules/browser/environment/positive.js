// @case description positive fixture for browser:browser.environment
// @tool glass-lint rules=browser:browser.environment
// @expect-error glass-lint rule=browser:browser.environment
navigator.userAgent;
// @expect-error glass-lint rule=browser:browser.environment
globalThis.navigator.userAgent;
// Window and worker-qualified navigator paths retain browser identity.
// @expect-error glass-lint rule=browser:browser.environment
window.navigator.userAgent;
// @expect-error glass-lint rule=browser:browser.environment
window.navigator.connection.effectiveType;
// @expect-error glass-lint rule=browser:browser.environment
self.navigator.language;
// @expect-error glass-lint rule=browser:browser.environment
self.navigator.hardwareConcurrency;
// Every configured property is a direct-read heuristic.
// @expect-error glass-lint rule=browser:browser.environment
navigator.language;
// @expect-error glass-lint rule=browser:browser.environment
screen.width;
// @expect-error glass-lint rule=browser:browser.environment
screen.availWidth;
// @expect-error glass-lint rule=browser:browser.environment
screen.availHeight;
// @expect-error glass-lint rule=browser:browser.environment
screen.colorDepth;
// @expect-error glass-lint rule=browser:browser.environment
screen.pixelDepth;
// Rooted window.screen reads preserve the receiver identity.
// @expect-error glass-lint rule=browser:browser.environment
window.screen.width;
// @expect-error glass-lint rule=browser:browser.environment
window.screen.availWidth;
// @expect-error glass-lint rule=browser:browser.environment
window.screen.colorDepth;
// @expect-error glass-lint rule=browser:browser.environment
navigator.languages;
// @expect-error glass-lint rule=browser:browser.environment
navigator.hardwareConcurrency;
// @expect-error glass-lint rule=browser:browser.environment
navigator.deviceMemory;
// @expect-error glass-lint rule=browser:browser.environment
navigator.vendor;
// @expect-error glass-lint rule=browser:browser.environment
navigator.cookieEnabled;
// @expect-error glass-lint rule=browser:browser.environment
navigator.maxTouchPoints;
// @expect-error glass-lint rule=browser:browser.environment
navigator.doNotTrack;
// @expect-error glass-lint rule=browser:browser.environment
navigator.webdriver;
// @expect-error glass-lint rule=browser:browser.environment
navigator.pdfViewerEnabled;
// @expect-error glass-lint rule=browser:browser.environment
navigator.onLine;
// @expect-error glass-lint rule=browser:browser.environment
navigator.connection.effectiveType;
// @expect-error glass-lint rule=browser:browser.environment
navigator.connection.rtt;
// @expect-error glass-lint rule=browser:browser.environment
navigator.connection.downlink;
// @expect-error glass-lint rule=browser:browser.environment
navigator.connection.saveData;

// Rooted navigator reads reject shadowed local lookalikes.
function inspect(navigator) {
    // @expect-no-error glass-lint rule=browser:browser.environment
    navigator.userAgent;
}
