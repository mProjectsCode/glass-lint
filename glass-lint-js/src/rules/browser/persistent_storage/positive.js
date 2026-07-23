// @case description positive fixture for browser:browser.persistent-storage
// @tool glass-lint rules=browser:browser.persistent-storage
// @expect-error glass-lint rule=browser:browser.persistent-storage
localStorage.getItem("x");
// @expect-error glass-lint rule=browser:browser.persistent-storage
globalThis.localStorage.getItem("global");
// The rule covers both configured key-value stores and database/cache opens.
// @expect-error glass-lint rule=browser:browser.persistent-storage
sessionStorage.setItem("other", "value");
// @expect-error glass-lint rule=browser:browser.persistent-storage
indexedDB.open("database");
// @expect-error glass-lint rule=browser:browser.persistent-storage
caches.open("cache");
const storage = localStorage;
// Aliases retain browser storage provenance.
// @expect-error glass-lint rule=browser:browser.persistent-storage
storage.setItem("alias", "value");
// @expect-error glass-lint rule=browser:browser.persistent-storage
localStorage.removeItem("x");
// @expect-error glass-lint rule=browser:browser.persistent-storage
sessionStorage.clear();
// @expect-error glass-lint rule=browser:browser.persistent-storage
sessionStorage.key(0);
// @expect-error glass-lint rule=browser:browser.persistent-storage
caches.match("/asset");
// @expect-error glass-lint rule=browser:browser.persistent-storage
caches.has("cache");
// @expect-error glass-lint rule=browser:browser.persistent-storage
caches.delete("cache");
// @expect-error glass-lint rule=browser:browser.persistent-storage
caches.keys();
// @expect-error glass-lint rule=browser:browser.persistent-storage
document.cookie;
// @expect-error glass-lint rule=browser:browser.persistent-storage
navigator.storage.persist();
// @expect-error glass-lint rule=browser:browser.persistent-storage
navigator.storage.persisted();
// @expect-error glass-lint rule=browser:browser.persistent-storage
navigator.storage.estimate();
// @expect-error glass-lint rule=browser:browser.persistent-storage
navigator.storage.getDirectory();
// Qualified navigator storage roots retain identity.
// @expect-error glass-lint rule=browser:browser.persistent-storage
window.navigator.storage.persist();
// @expect-error glass-lint rule=browser:browser.persistent-storage
self.navigator.storage.estimate();
// @expect-error glass-lint rule=browser:browser.persistent-storage
const opfs = navigator.storage.getDirectory();
// Returned OPFS directory handles retain storage provenance.
// @expect-error glass-lint rule=browser:browser.persistent-storage
opfs.getFileHandle("data.bin");
// @expect-error glass-lint rule=browser:browser.persistent-storage
opfs.removeEntry("old.bin");
// @expect-error glass-lint rule=browser:browser.persistent-storage
const windowOpfs = window.navigator.storage.getDirectory();
// @expect-error glass-lint rule=browser:browser.persistent-storage
windowOpfs.getDirectoryHandle("nested");
// @expect-error glass-lint rule=browser:browser.persistent-storage
indexedDB.deleteDatabase("database");
// @expect-error glass-lint rule=browser:browser.persistent-storage
indexedDB.databases();
// Window and worker-qualified storage roots preserve browser identity.
// @expect-error glass-lint rule=browser:browser.persistent-storage
window.localStorage.getItem("window");
// @expect-error glass-lint rule=browser:browser.persistent-storage
window.sessionStorage.setItem("window", "value");
// @expect-error glass-lint rule=browser:browser.persistent-storage
window.indexedDB.open("window-database");
// @expect-error glass-lint rule=browser:browser.persistent-storage
window.caches.match("/window-asset");
// @expect-error glass-lint rule=browser:browser.persistent-storage
self.caches.open("worker-cache");
// Cookie Store operations are persistent browser storage access too.
// @expect-error glass-lint rule=browser:browser.persistent-storage
window.cookieStore.get("session");
// @expect-error glass-lint rule=browser:browser.persistent-storage
window.cookieStore.getAll();
// @expect-error glass-lint rule=browser:browser.persistent-storage
window.cookieStore.set("session", "value");
// @expect-error glass-lint rule=browser:browser.persistent-storage
window.cookieStore.delete("session");
// @expect-error glass-lint rule=browser:browser.persistent-storage
globalThis.cookieStore.get("global");
// Worker-qualified Cookie Store operations retain browser identity.
// @expect-error glass-lint rule=browser:browser.persistent-storage
self.cookieStore.getAll();
