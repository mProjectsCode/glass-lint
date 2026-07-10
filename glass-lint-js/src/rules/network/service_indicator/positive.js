// @case description positive fixture for js:network.service-indicator
// @tool glass-lint rules=js:network.service-indicator
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import x from "openai";
// second independent example

// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
const serviceEndpoint = "supabase.co";

// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import firebase from "firebase";
// Migrated: network/string-literal-markers.js

// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
const openAiEndpoint = "https://api.openai.com/v1/chat/completions";

// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
const templatedEndpoint = `https://api.openai.com/v1/${resource}`;
