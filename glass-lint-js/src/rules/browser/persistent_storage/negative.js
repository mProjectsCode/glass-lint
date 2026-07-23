// @case description negative fixture for browser:browser.persistent-storage
// @tool glass-lint rules=browser:browser.persistent-storage
// @expect-no-error glass-lint rule=browser:browser.persistent-storage
// A local same-shaped object is not the browser global.
const localStorage = { getItem() {} };
// @expect-no-error glass-lint rule=browser:browser.persistent-storage
localStorage.getItem("local");

// Unlisted methods and reassigned aliases are excluded.
globalThis.localStorage.getAll("x");
let storage = globalThis.sessionStorage;
storage = { setItem() {} };
// @expect-no-error glass-lint rule=browser:browser.persistent-storage
storage.setItem("local", "value");

// Dynamic and local storage lookalikes remain excluded.
function readCookie(document) {
    // @expect-no-error glass-lint rule=browser:browser.persistent-storage
    document.cookie;
}
const storageApi = { persist() {} };
// @expect-no-error glass-lint rule=browser:browser.persistent-storage
storageApi.persist();

function localWindow(window) {
    // @expect-no-error glass-lint rule=browser:browser.persistent-storage
    window.cookieStore.get("local");
    // @expect-no-error glass-lint rule=browser:browser.persistent-storage
    window.localStorage.getItem("local");
    // @expect-no-error glass-lint rule=browser:browser.persistent-storage
    window.caches.open("local");
}
localWindow({ cookieStore: { get() {} } });

function localSelf(self) {
    // @expect-no-error glass-lint rule=browser:browser.persistent-storage
    self.caches.open("local");
    // @expect-no-error glass-lint rule=browser:browser.persistent-storage
    self.cookieStore.get("local");
}
localSelf({ caches: { open() {} }, cookieStore: { get() {} } });

function localWindowNavigator(window) {
    // @expect-no-error glass-lint rule=browser:browser.persistent-storage
    window.navigator.storage.persist();
}
localWindowNavigator({ navigator: { storage: { persist() {} } } });

function localStorageNavigator(window) {
    const directory = window.navigator.storage.getDirectory();
    // @expect-no-error glass-lint rule=browser:browser.persistent-storage
    directory.getFileHandle("local.bin");
}
localStorageNavigator({ navigator: { storage: { getDirectory() { return {}; } } } });
