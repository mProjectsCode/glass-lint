// @case description positive fixture for js:network.service-indicator
// @tool glass-lint rules=js:network.service-indicator
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
import x from "openai";
// second independent example
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
const serviceEndpoint = "supabase.co";
