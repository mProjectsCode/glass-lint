// @case description A plugin adds a ribbon action
// @tool glass-lint config=heuristic
// @tool eslint-obsidianmd config=recommended
// @expect-error glass-lint rule=obsidian:ui.status-bar count=1 line=any
// @expect-error glass-lint rule=obsidian:ui.ribbon count=1 line=any
// @expect-error glass-lint rule=obsidian:ui.command count=1 line=any

import { Plugin } from "obsidian";

export default class RibbonPlugin extends Plugin {
  onload() {
    this.rolls = [];
    this.status = this.addStatusBarItem();
    this.ribbon = this.addRibbonIcon("dice", "Roll dice", () => this.roll());
    this.ribbon.addClass("dice-roller-ribbon");
    this.addCommand({
      id: "roll-dice",
      name: "Roll dice",
      callback: () => this.roll(),
    });
    this.updateStatus();
  }

  roll() {
    const value = this.randomInt(1, 6);
    this.rolls.push({ value, time: Date.now() });
    if (this.rolls.length > 20) this.rolls.shift();
    this.updateStatus();
    return value;
  }

  randomInt(minimum, maximum) {
    const span = maximum - minimum + 1;
    return minimum + Math.floor(Math.random() * span);
  }

  updateStatus() {
    const latest = this.rolls.at(-1);
    if (!latest) {
      this.status.setText("Dice: not rolled");
      return;
    }
    const average = this.rolls.reduce((sum, roll) => sum + roll.value, 0) / this.rolls.length;
    this.status.setText(`Dice: ${latest.value} (avg ${average.toFixed(1)})`);
  }

  onunload() {
    this.rolls.length = 0;
    this.status = null;
  }
}
