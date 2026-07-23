// @case description negative fixture for js:dynamic-code.string-timer
// @tool glass-lint rules=js:dynamic-code.string-timer
// @expect-no-error glass-lint rule=js:dynamic-code.string-timer
// Function callbacks and dynamic values are not string-code timers.
// @expect-no-error glass-lint rule=js:dynamic-code.string-timer
setTimeout(() => {}, 10);
setTimeout(code, 10);

// A shadowed global and a reassigned alias are excluded.
function localTimer(setTimeout) {
  setTimeout("code()", 10);
}
let schedule = globalThis.setTimeout;
schedule = safeSchedule;
// @expect-no-error glass-lint rule=js:dynamic-code.string-timer
schedule("code()", 10);

function localWindow(window) {
  // @expect-no-error glass-lint rule=js:dynamic-code.string-timer
  window.setInterval("localCode()", 10);
}
localWindow({ setInterval() {} });
globalThis.setTimeout = safeSchedule;
// @expect-no-error glass-lint rule=js:dynamic-code.string-timer
globalThis.setTimeout("mutated", 10);
