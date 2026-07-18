// @case description negative fixture for browser:browser.environment
// @tool glass-lint rules=browser:browser.environment
// @expect-no-error glass-lint rule=browser:browser.environment message_id=detected
// Unlisted environment properties are ignored.
// @expect-no-error glass-lint rule=browser:browser.environment message_id=detected
screen.orientation;
function localEnvironment(navigator) {
    // @expect-no-error glass-lint rule=browser:browser.environment message_id=detected
    navigator.languages;
    // @expect-no-error glass-lint rule=browser:browser.environment message_id=detected
    navigator.connection.effectiveType;
}

function localScreen(screen) {
    // @expect-no-error glass-lint rule=browser:browser.environment message_id=detected
    screen.width;
}

function localWindow(window) {
    // @expect-no-error glass-lint rule=browser:browser.environment message_id=detected
    window.screen.width;
    // @expect-no-error glass-lint rule=browser:browser.environment message_id=detected
    window.navigator.userAgent;
}

function localSelf(self) {
    // @expect-no-error glass-lint rule=browser:browser.environment message_id=detected
    self.navigator.language;
}

// Dynamic property names are outside this direct-chain heuristic.
function read(navigator, property) {
    // @expect-no-error glass-lint rule=browser:browser.environment message_id=detected
    navigator[property];
}
