// @case description A Reading Room plugin connects Obsidian data with browser-side flows
// @case tags obsidian,flow,aliases,certainty
// @tool glass-lint rules=browser:browser.file-dialog,browser:dom.remote-resource,browser:dynamic-code.script-injection,obsidian:lifecycle.events,obsidian:network.request,obsidian:ui.command,obsidian:ui.status-bar,obsidian:vault.read,obsidian:workspace.active-file
// @expect-error glass-lint rule=browser:browser.file-dialog count=1 line=116 certainty=definite
// @expect-error glass-lint rule=browser:dom.remote-resource count=1 line=79 certainty=definite
// @expect-error glass-lint rule=browser:dom.remote-resource count=1 line=93 certainty=definite
// Both script-injection sinks share one rule evidence group; the optional
// path makes that aggregate finding possible even though the direct helper
// flow is complete on its own.
// @expect-error glass-lint rule=browser:dynamic-code.script-injection count=1 line=89 certainty=possible
// @expect-error glass-lint rule=browser:dynamic-code.script-injection count=1 line=103 certainty=possible
// @expect-error glass-lint rule=obsidian:lifecycle.events count=1 line=any
// @expect-error glass-lint rule=obsidian:network.request count=1 line=any
// @expect-error glass-lint rule=obsidian:ui.command count=1 line=any
// @expect-error glass-lint rule=obsidian:ui.status-bar count=1 line=any
// @expect-error glass-lint rule=obsidian:vault.read count=1 line=any
// @expect-error glass-lint rule=obsidian:workspace.active-file count=2 line=any

import { Plugin, requestUrl } from "obsidian";

// The fake plugin keeps a small reading list in the active note, fetches a
// cover image, and adds a couple of optional browser enhancements.
export default class ReadingRoomPlugin extends Plugin {
  async onload() {
    this.addCommand({
      id: "refresh-reading-room",
      name: "Refresh reading room",
      callback: () => this.refreshReadingRoom(),
    });
    this.status = this.addStatusBarItem();
    this.registerEvent(
      this.app.vault.on("modify", (file) => this.refreshIfActive(file)),
    );
    await this.refreshReadingRoom();
  }

  async refreshReadingRoom() {
    const activeFile = this.app.workspace.getActiveFile();
    if (!activeFile || activeFile.extension !== "md") return;

    const markdown = await this.app.vault.cachedRead(activeFile);
    const readingList = this.extractReadingList(markdown);
    const cover = await this.fetchCover(readingList[0]);

    this.renderCover(cover);
    this.installHighlight();
    this.installOptionalFormatter();
    this.addAttachmentPicker();
    this.status.setText(`${readingList.length} books in the room`);
  }

  async fetchCover(book) {
    if (!book) return null;

    const response = await requestUrl({
      url: `https://covers.example.test/${encodeURIComponent(book)}.json`,
      method: "GET",
    });
    return response.json;
  }

  extractReadingList(markdown) {
    return markdown
      .split("\n")
      .filter((line) => line.startsWith("- "))
      .map((line) => line.slice(2).trim());
  }

  renderCover(cover) {
    if (!cover) return;

    // The alias and setAttribute call are both part of the same object flow.
    const coverImage = document.createElement("img");
    const previewImage = coverImage;
    previewImage.setAttribute(
      "src",
      "https://images.example.test/reading-room-cover.png",
    );
    appendPreview(previewImage);
  }

  installOptionalFormatter() {
    // If the setting is enabled, one modeled path configures this script;
    // the unconditional sink therefore reports a possible-path finding.
    const formatter = document.createElement("script");
    if (this.settings?.experimentalFormatter) {
      formatter.textContent = "window.readingRoomFormatter = true;";
    }
    document.head.appendChild(formatter);

    const stylesheet = document.createElement("link");
    stylesheet.href = "https://cdn.example.test/reading-room.css";
    document.head.appendChild(stylesheet);

  }

  installHighlight() {
    // A separate helper takes the direct alias path. Its flow is independent
    // of the optional formatter and is complete on its own.
    const helper = document.createElement("script");
    const inlineScript = helper;
    inlineScript.textContent = "window.readingRoomHighlight = true;";
    document.body.appendChild(inlineScript);
  }

  addAttachmentPicker() {
    const input = document.createElement("input");
    const attachmentPicker = input;
    attachmentPicker.accept = "image/*";
    attachmentPicker.addEventListener("change", (event) => {
      this.attachFile(event.target.files[0]);
    });
    attachmentPicker.click();
    // Keep configuration as the final step in this small flow so the
    // finding points at the moment the picker becomes a file dialog.
    attachmentPicker.setAttribute("type", "file");
  }

  refreshIfActive(file) {
    const activeFile = this.app.workspace.getActiveFile();
    if (activeFile?.path === file.path) this.refreshReadingRoom();
  }

  attachFile(file) {
    if (!file) return;
    console.log(`Attached ${file.name}`);
  }

  onunload() {
    this.status?.setText("");
  }
}

function appendPreview(element) {
  document.body.append(element);
}

// These examples are intentionally not findings: a local asset is not a
// remote resource, and a dynamic value is not enough to prove one.
const localIcon = document.createElement("img");
localIcon.src = "/icons/book.png";
document.body.appendChild(localIcon);

const dynamicPreview = document.createElement("img");
dynamicPreview.src = getConfiguredPreviewUrl();
document.body.appendChild(dynamicPreview);
