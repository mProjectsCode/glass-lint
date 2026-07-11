// @case description all configured Platform flags and module aliases
// @tool glass-lint rules=obsidian:platform.branching
import * as obsidian from "obsidian";
// @expect-error glass-lint rule=obsidian:platform.branching message_id=detected
obsidian.Platform.isMobile;
// @expect-error glass-lint rule=obsidian:platform.branching message_id=detected
obsidian.Platform.isDesktop;
// @expect-error glass-lint rule=obsidian:platform.branching message_id=detected
obsidian.Platform.isIosApp;
// @expect-error glass-lint rule=obsidian:platform.branching message_id=detected
obsidian.Platform.isAndroidApp;
// @expect-error glass-lint rule=obsidian:platform.branching message_id=detected
obsidian.Platform.isMacOS;
// @expect-error glass-lint rule=obsidian:platform.branching message_id=detected
obsidian.Platform.isWin;
// @expect-error glass-lint rule=obsidian:platform.branching message_id=detected
obsidian.Platform.isLinux;

// Optional and static computed accesses retain module provenance.
// @expect-error glass-lint rule=obsidian:platform.branching message_id=detected
obsidian?.Platform?.[`isMobile`];
// @expect-error glass-lint rule=obsidian:platform.branching message_id=detected
if (obsidian.Platform["isMobile"]) console.log("mobile");

const namespaceAlias = obsidian;
// @expect-error glass-lint rule=obsidian:platform.branching message_id=detected
namespaceAlias.Platform.isDesktop;
