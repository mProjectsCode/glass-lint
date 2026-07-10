// @case description negative fixture for obsidian:ui.settings-tab
// @tool glass-lint rules=obsidian:ui.settings-tab
// @expect-no-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
function localLookalike() { return null; }
localLookalike();
// @expect-no-error glass-lint rule=obsidian:ui.settings-tab message_id=detected
this.addSettingsTab(tab);

// Migrated: interface/local-classes-ignored.js and unused-imports-ignored.js
import { Setting as LegacyUnusedSetting } from "obsidian";
class LegacyLocalSetting {}
new LegacyLocalSetting();
LegacyUnusedSetting;
