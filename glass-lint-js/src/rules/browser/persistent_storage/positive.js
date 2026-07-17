// @case description positive fixture for browser:browser.persistent-storage
// @tool glass-lint rules=browser:browser.persistent-storage
// @expect-error glass-lint rule=browser:browser.persistent-storage message_id=detected
localStorage.getItem("x");
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
