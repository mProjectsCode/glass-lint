// @case description Ported old classifier regression: metadata flow should not match local lookalikes or sibling bindings
// @tool glass-lint rules=obsidian:metadata.frontmatter,obsidian:metadata.traversal,obsidian:metadata.extraction

console.log(settings.frontmatter);

const links = this.app.metadataCache.resolvedLinks;
Object.entries(settings);

const localModel = { tags: [], links: [] };
localModel.tags;

function captureCache() {
  const cache = this.app.metadataCache.getFileCache(file);
}
function useUnrelated() {
  const cache = localModel;
  cache.tags;
}
