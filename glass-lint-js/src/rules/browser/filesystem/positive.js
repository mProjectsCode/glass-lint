// @case description rooted File System Access directory picker
// @tool glass-lint rules=browser:browser.filesystem
// @expect-error glass-lint rule=browser:browser.filesystem message_id=detected
window.showDirectoryPicker();
const browserWindow = window;
// @expect-error glass-lint rule=browser:browser.filesystem message_id=detected
browserWindow["showDirectoryPicker"]();
// @expect-error glass-lint rule=browser:browser.filesystem message_id=detected
const directory = window.showDirectoryPicker();
// Returned directory handles retain the picker provenance.
// @expect-error glass-lint rule=browser:browser.filesystem message_id=detected
directory.getFileHandle("notes.md");
// @expect-error glass-lint rule=browser:browser.filesystem message_id=detected
directory.getDirectoryHandle("attachments");
// @expect-error glass-lint rule=browser:browser.filesystem message_id=detected
directory.removeEntry("old.md");
// @expect-error glass-lint rule=browser:browser.filesystem message_id=detected
directory.queryPermission();
// @expect-error glass-lint rule=browser:browser.filesystem message_id=detected
directory.entries();
// @expect-error glass-lint rule=browser:browser.filesystem message_id=detected
directory.isSameEntry(otherDirectory);
// @expect-error glass-lint rule=browser:browser.filesystem message_id=detected
self.showDirectoryPicker();
// @expect-error glass-lint rule=browser:browser.filesystem message_id=detected
const workerDirectory = self.showDirectoryPicker();
// @expect-error glass-lint rule=browser:browser.filesystem message_id=detected
workerDirectory.resolve("child");
// @expect-error glass-lint rule=browser:browser.filesystem message_id=detected
globalThis.showDirectoryPicker();
