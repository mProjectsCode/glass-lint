// @case description negative fixture for browser:browser.persistent-storage
// @tool glass-lint rules=browser:browser.persistent-storage
// @expect-no-error glass-lint rule=browser:browser.persistent-storage message_id=detected
// A local same-shaped object is not the browser global.
const localStorage = { getItem() {} };
// @expect-no-error glass-lint rule=browser:browser.persistent-storage message_id=detected
localStorage.getItem("local");

// Unlisted methods and reassigned aliases are excluded.
globalThis.localStorage.removeItem("x");
let storage = globalThis.sessionStorage;
storage = { setItem() {} };
// @expect-no-error glass-lint rule=browser:browser.persistent-storage message_id=detected
storage.setItem("local", "value");
