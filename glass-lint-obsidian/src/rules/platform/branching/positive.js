// @case description all configured Platform flags and module aliases
// @tool glass-lint rules=obsidian:platform.branching
import * as obsidian from "obsidian";
// @expect-error glass-lint rule=obsidian:platform.branching
obsidian.Platform.isMobile;
// @expect-error glass-lint rule=obsidian:platform.branching
obsidian.Platform.isDesktop;
// @expect-error glass-lint rule=obsidian:platform.branching
obsidian.Platform.isIosApp;
// @expect-error glass-lint rule=obsidian:platform.branching
obsidian.Platform.isAndroidApp;
// @expect-error glass-lint rule=obsidian:platform.branching
obsidian.Platform.isMacOS;
// @expect-error glass-lint rule=obsidian:platform.branching
obsidian.Platform.isWin;
// @expect-error glass-lint rule=obsidian:platform.branching
obsidian.Platform.isLinux;

// Optional and static computed accesses retain module provenance.
// @expect-error glass-lint rule=obsidian:platform.branching
obsidian?.Platform?.[`isMobile`];
// @expect-error glass-lint rule=obsidian:platform.branching
if (obsidian.Platform["isMobile"]) console.log("mobile");

const namespaceAlias = obsidian;
// @expect-error glass-lint rule=obsidian:platform.branching
namespaceAlias.Platform.isDesktop;
// @expect-error glass-lint rule=obsidian:platform.branching
obsidian.Platform.isDesktopApp;
// @expect-error glass-lint rule=obsidian:platform.branching
obsidian.Platform.isMobileApp;
// @expect-error glass-lint rule=obsidian:platform.branching
obsidian.Platform.isPhone;
// @expect-error glass-lint rule=obsidian:platform.branching
obsidian.Platform.isTablet;
// @expect-error glass-lint rule=obsidian:platform.branching
obsidian.Platform.isSafari;
// @expect-error glass-lint rule=obsidian:platform.branching
obsidian.Platform.resourcePathPrefix;
