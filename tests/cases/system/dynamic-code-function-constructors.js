// @case description Function constructor variants are detected as dynamic code
// @tool glass-lint rules=obsidian:dynamic_code
// @tool eslint-obsidianmd config=recommended

Function("return 1")(); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
new Function("return 1")(); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
const F = Function; F("return 1")(); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
(function () {}).constructor("return 1")(); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
const AsyncFunction = async function () {}.constructor; new AsyncFunction("return 1"); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
const GeneratorFunction = (function* () {}).constructor; GeneratorFunction("yield 1"); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
const AsyncGeneratorFunction = (async function* () {}).constructor; new AsyncGeneratorFunction("yield 1"); // @expect-error glass-lint rule=obsidian:dynamic_code message_id=detected
