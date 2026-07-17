// @case description TypeScript runtime and type-only input
// @case tags typescript,network
// @tool glass-lint rules=js:network.request
// @tool eslint-obsidianmd config=recommended

interface RequestShape { url: string }
type FetchType = typeof fetch;
import type { fetch as ImportedFetch } from "api";

const request = (value: RequestShape): ReturnType<FetchType> =>
    fetch(value.url); // @expect-error glass-lint rule=js:network.request message_id=detected

declare const fetchTypeOnly: ImportedFetch;
void fetchTypeOnly;
