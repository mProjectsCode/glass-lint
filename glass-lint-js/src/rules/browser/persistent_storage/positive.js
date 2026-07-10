// @case description positive fixture for js:browser.persistent-storage
// @tool glass-lint rules=js:browser.persistent-storage
// @expect-error glass-lint rule=js:browser.persistent-storage message_id=detected
localStorage.getItem("x");
// The rule covers both configured key-value stores and database/cache opens.
// @expect-error glass-lint rule=js:browser.persistent-storage message_id=detected
sessionStorage.setItem("other", "value");
// @expect-error glass-lint rule=js:browser.persistent-storage message_id=detected
indexedDB.open("database");
// @expect-error glass-lint rule=js:browser.persistent-storage message_id=detected
caches.open("cache");
const storage = localStorage;
// Aliases retain browser storage provenance.
// @expect-error glass-lint rule=js:browser.persistent-storage message_id=detected
storage.setItem("alias", "value");
