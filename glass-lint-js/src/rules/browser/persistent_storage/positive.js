// @case description positive fixture for js:browser.persistent-storage
// @tool glass-lint rules=js:browser.persistent-storage
// @expect-error glass-lint rule=js:browser.persistent-storage message_id=detected
localStorage.getItem("x");
// second independent example
// @expect-error glass-lint rule=js:browser.persistent-storage message_id=detected
sessionStorage.getItem("other");
const storage = localStorage;
// @expect-error glass-lint rule=js:browser.persistent-storage message_id=detected
storage.setItem("alias", "value");
