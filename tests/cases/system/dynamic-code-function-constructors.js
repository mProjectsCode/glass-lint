// @case description Ported old classifier cases: Function constructor variants
// @tool glass-lint rules=obsidian:dynamic_code

Function("return 1")();
const F = Function; F("return 1")();
(function () {}).constructor("return 1")();
const AsyncFunction = async function () {}.constructor; new AsyncFunction("return 1");
const GeneratorFunction = (function* () {}).constructor; GeneratorFunction("yield 1");
const AsyncGeneratorFunction = (async function* () {}).constructor; new AsyncGeneratorFunction("yield 1");
