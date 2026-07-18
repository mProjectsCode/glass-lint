// @case description WebHID, Web Serial, and WebUSB requests
// @tool glass-lint rules=browser:browser.permissions-hardware
// @expect-error glass-lint rule=browser:browser.permissions-hardware message_id=detected
navigator.hid.requestDevice({ filters: [] });
// @expect-error glass-lint rule=browser:browser.permissions-hardware message_id=detected
window.navigator.hid.requestDevice({ filters: [] });
// @expect-error glass-lint rule=browser:browser.permissions-hardware message_id=detected
self.navigator.serial.requestPort();
// @expect-error glass-lint rule=browser:browser.permissions-hardware message_id=detected
globalThis.navigator.usb.requestDevice({ filters: [] });
// @expect-error glass-lint rule=browser:browser.permissions-hardware message_id=detected
navigator.serial.requestPort();
// @expect-error glass-lint rule=browser:browser.permissions-hardware message_id=detected
navigator.usb.requestDevice({ filters: [] });

const usb = this.navigator.usb;
// @expect-error glass-lint rule=browser:browser.permissions-hardware message_id=detected
usb.requestDevice({ filters: [] });
