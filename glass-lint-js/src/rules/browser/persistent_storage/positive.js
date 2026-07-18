// @case description positive fixture for browser:browser.persistent-storage
// @tool glass-lint rules=browser:browser.persistent-storage
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
localStorage.getItem("x");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
globalThis.localStorage.getItem("global");
// The rule covers both configured key-value stores and database/cache opens.
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
sessionStorage.setItem("other", "value");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
indexedDB.open("database");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
caches.open("cache");
const storage = localStorage;
// Aliases retain browser storage provenance.
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
storage.setItem("alias", "value");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
localStorage.removeItem("x");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
sessionStorage.clear();
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
sessionStorage.key(0);
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
caches.match("/asset");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
caches.has("cache");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
caches.delete("cache");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
caches.keys();
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
document.cookie;
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
navigator.storage.persist();
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
navigator.storage.persisted();
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
navigator.storage.estimate();
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
navigator.storage.getDirectory();
// Qualified navigator storage roots retain identity.
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
window.navigator.storage.persist();
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
self.navigator.storage.estimate();
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
const opfs = navigator.storage.getDirectory();
// Returned OPFS directory handles retain storage provenance.
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
opfs.getFileHandle("data.bin");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
opfs.removeEntry("old.bin");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
const windowOpfs = window.navigator.storage.getDirectory();
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
windowOpfs.getDirectoryHandle("nested");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
indexedDB.deleteDatabase("database");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
indexedDB.databases();
// Window and worker-qualified storage roots preserve browser identity.
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
window.localStorage.getItem("window");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
window.sessionStorage.setItem("window", "value");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
window.indexedDB.open("window-database");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
window.caches.match("/window-asset");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
self.caches.open("worker-cache");
// Cookie Store operations are persistent browser storage access too.
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
window.cookieStore.get("session");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
window.cookieStore.getAll();
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
window.cookieStore.set("session", "value");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
window.cookieStore.delete("session");
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
globalThis.cookieStore.get("global");
// Worker-qualified Cookie Store operations retain browser identity.
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
self.cookieStore.getAll();
