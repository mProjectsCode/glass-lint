// @case description negative fixture for js:dynamic-code.string-timer
// @tool glass-lint rules=js:dynamic-code.string-timer
// @expect-no-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
// Function callbacks and dynamic values are not string-code timers.
// @expect-no-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
setTimeout(() => {}, 10);
setTimeout(code, 10);

// A shadowed global and a reassigned alias are excluded.
function localTimer(setTimeout) {
  setTimeout("code()", 10);
}
let schedule = globalThis.setTimeout;
schedule = safeSchedule;
// @expect-no-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
schedule("code()", 10);

function localWindow(window) {
  // @expect-no-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
  window.setInterval("localCode()", 10);
}
localWindow({ setInterval() {} });
globalThis.setTimeout = safeSchedule;
// @expect-no-error glass-lint rule=js:dynamic-code.string-timer message_id=detected
globalThis.setTimeout("mutated", 10);
