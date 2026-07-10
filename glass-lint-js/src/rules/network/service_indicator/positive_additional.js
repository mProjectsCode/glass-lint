// @case description additional literal coverage for js:network.service-indicator
// @tool glass-lint rules=js:network.service-indicator
// The remaining configured endpoint markers are each reported.
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
const openAiEndpoint = "https://api.openai.com/v1/chat/completions";
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
const supabaseEndpoint = "https://project.supabase.co/rest/v1";

// A matching static template fragment is still literal evidence.
// @expect-error glass-lint rule=js:network.service-indicator message_id=detected
const templatedEndpoint = `https://api.openai.com/v1/${resource}`;
