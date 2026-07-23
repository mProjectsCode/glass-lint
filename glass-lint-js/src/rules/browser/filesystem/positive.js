// @case description rooted File System Access directory picker
// @tool glass-lint rules=browser:browser.filesystem
// @expect-error glass-lint rule=browser:browser.filesystem
window.showDirectoryPicker();
const browserWindow = window;
// @expect-error glass-lint rule=browser:browser.filesystem
browserWindow["showDirectoryPicker"]();
// @expect-error glass-lint rule=browser:browser.filesystem
const directory = window.showDirectoryPicker();
// Returned directory handles retain the picker provenance.
// @expect-error glass-lint rule=browser:browser.filesystem
directory.getFileHandle("notes.md");
// @expect-error glass-lint rule=browser:browser.filesystem
directory.getDirectoryHandle("attachments");
// @expect-error glass-lint rule=browser:browser.filesystem
directory.removeEntry("old.md");
// @expect-error glass-lint rule=browser:browser.filesystem
directory.queryPermission();
// @expect-error glass-lint rule=browser:browser.filesystem
directory.entries();
// @expect-error glass-lint rule=browser:browser.filesystem
directory.isSameEntry(otherDirectory);
// @expect-error glass-lint rule=browser:browser.filesystem
self.showDirectoryPicker();
// @expect-error glass-lint rule=browser:browser.filesystem
const workerDirectory = self.showDirectoryPicker();
// @expect-error glass-lint rule=browser:browser.filesystem
workerDirectory.resolve("child");
// @expect-error glass-lint rule=browser:browser.filesystem
globalThis.showDirectoryPicker();
