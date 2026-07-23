// @case description similar, shadowed, dynamic, reassigned, and unlisted flags
// @tool glass-lint rules=obsidian:platform.branching
import * as localObsidian from 'obsidian-utils';
// @expect-no-error glass-lint rule=obsidian:platform.branching
localObsidian.Platform.isMobile;

function shadowedNamespace(obsidian) {
    // @expect-no-error glass-lint rule=obsidian:platform.branching
    obsidian.Platform.isMobile;
}

function dynamicProperty(flag) {
    // @expect-no-error glass-lint rule=obsidian:platform.branching
    obsidian.Platform[flag];
}

let namespace = require('obsidian');
namespace = localObsidian;
// @expect-no-error glass-lint rule=obsidian:platform.branching
namespace.Platform.isLinux;

// @expect-no-error glass-lint rule=obsidian:platform.branching
obsidian.Platform.isTablet;
