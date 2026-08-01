// @case description positive fixture for browser:browser.worker
// @tool glass-lint rules=browser:browser.worker

// @expect-error glass-lint rule=browser:browser.worker
const worker = new Worker("worker.js");
// @expect-error glass-lint rule=browser:browser.worker
const shared = new SharedWorker("shared-worker.js");

// Service-worker registration loads code in a separate execution context.
// @expect-error glass-lint rule=browser:browser.worker
navigator.serviceWorker.register("service-worker.js");
// @expect-error glass-lint rule=browser:browser.worker
navigator.serviceWorker.getRegistration();
// @expect-error glass-lint rule=browser:browser.worker
navigator.serviceWorker.getRegistrations();

// CSS worklets load module code outside the current realm.
// @expect-error glass-lint rule=browser:browser.worker
CSS.paintWorklet.addModule("paint.js");
// @expect-error glass-lint rule=browser:browser.worker
CSS.layoutWorklet.addModule("layout.js");
// @expect-error glass-lint rule=browser:browser.worker
CSS.animationWorklet.addModule("animation.js");

// @expect-error glass-lint rule=browser:browser.worker
importScripts("worker-helper.js");
