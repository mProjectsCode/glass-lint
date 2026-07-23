// @case description WebHID, Web Serial, and WebUSB requests
// @tool glass-lint rules=browser:browser.permissions-hardware
// @expect-error glass-lint rule=browser:browser.permissions-hardware
navigator.hid.requestDevice({ filters: [] });
// @expect-error glass-lint rule=browser:browser.permissions-hardware
window.navigator.hid.requestDevice({ filters: [] });
// @expect-error glass-lint rule=browser:browser.permissions-hardware
self.navigator.serial.requestPort();
// @expect-error glass-lint rule=browser:browser.permissions-hardware
globalThis.navigator.usb.requestDevice({ filters: [] });
// @expect-error glass-lint rule=browser:browser.permissions-hardware
navigator.serial.requestPort();
// @expect-error glass-lint rule=browser:browser.permissions-hardware
navigator.usb.requestDevice({ filters: [] });

const usb = this.navigator.usb;
// @expect-error glass-lint rule=browser:browser.permissions-hardware
usb.requestDevice({ filters: [] });
