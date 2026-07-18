// @case description negative fixture for browser:browser.persistent-storage
// @tool glass-lint rules=browser:browser.persistent-storage
// @expect-no-error glass-lint rule=browser:browser.persistent-storage message_id=detected
// A local same-shaped object is not the browser global.
const localStorage = { getItem() {} };
// @expect-no-error glass-lint rule=browser:browser.persistent-storage message_id=detected
localStorage.getItem("local");

// Unlisted methods and reassigned aliases are excluded.
globalThis.localStorage.getAll("x");
let storage = globalThis.sessionStorage;
storage = { setItem() {} };
// @expect-no-error glass-lint rule=browser:browser.persistent-storage message_id=detected
storage.setItem("local", "value");

// Dynamic and local storage lookalikes remain excluded.
function readCookie(document) {
    // @expect-no-error glass-lint rule=browser:browser.persistent-storage message_id=detected
    document.cookie;
}
const storageApi = { persist() {} };
// @expect-no-error glass-lint rule=browser:browser.persistent-storage message_id=detected
storageApi.persist();

function localWindow(window) {
    // @expect-no-error glass-lint rule=browser:browser.persistent-storage message_id=detected
    window.cookieStore.get("local");
    // @expect-no-error glass-lint rule=browser:browser.persistent-storage message_id=detected
    window.localStorage.getItem("local");
    // @expect-no-error glass-lint rule=browser:browser.persistent-storage message_id=detected
    window.caches.open("local");
}
localWindow({ cookieStore: { get() {} } });

function localSelf(self) {
    // @expect-no-error glass-lint rule=browser:browser.persistent-storage message_id=detected
    self.caches.open("local");
    // @expect-no-error glass-lint rule=browser:browser.persistent-storage message_id=detected
    self.cookieStore.get("local");
}
localSelf({ caches: { open() {} }, cookieStore: { get() {} } });

function localWindowNavigator(window) {
    // @expect-no-error glass-lint rule=browser:browser.persistent-storage message_id=detected
    window.navigator.storage.persist();
}
localWindowNavigator({ navigator: { storage: { persist() {} } } });

function localStorageNavigator(window) {
    const directory = window.navigator.storage.getDirectory();
    // @expect-no-error glass-lint rule=browser:browser.persistent-storage message_id=detected
    directory.getFileHandle("local.bin");
}
localStorageNavigator({ navigator: { storage: { getDirectory() { return {}; } } } });
