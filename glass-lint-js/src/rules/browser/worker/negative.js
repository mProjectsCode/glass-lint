// @case description negative fixture for browser:browser.worker
// @tool glass-lint rules=browser:browser.worker

// Local constructors and APIs with the same names are not browser workers.
function localWorker(Worker, SharedWorker, importScripts) {
    // @expect-no-error glass-lint rule=browser:browser.worker
    new Worker("local.js");
    // @expect-no-error glass-lint rule=browser:browser.worker
    new SharedWorker("local.js");
    // @expect-no-error glass-lint rule=browser:browser.worker
    importScripts("local.js");
}
localWorker(class {}, class {}, () => {});

function localNavigator(navigator) {
    // @expect-no-error glass-lint rule=browser:browser.worker
    navigator.serviceWorker.register("local.js");
}
localNavigator({ serviceWorker: { register() {} } });

// Dynamic property selection cannot establish a rooted worklet API.
const worklet = getWorklet();
const method = getMethod();
// @expect-no-error glass-lint rule=browser:browser.worker
worklet[method]("local.js");
