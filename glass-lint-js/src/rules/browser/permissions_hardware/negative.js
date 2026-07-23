// @case description shadowed, dynamic, reassigned, and unsupported hardware calls
// @tool glass-lint rules=browser:browser.permissions-hardware
const navigator = { hid: { requestDevice() {} } };
// @expect-no-error glass-lint rule=browser:browser.permissions-hardware
navigator.hid.requestDevice({});

const property = getPropertyName();
// @expect-no-error glass-lint rule=browser:browser.permissions-hardware
globalThis.navigator.usb[property]();
// @expect-no-error glass-lint rule=browser:browser.permissions-hardware
navigator.serial.getPorts();

let serial = globalThis.navigator.serial;
serial = localSerial;
// @expect-no-error glass-lint rule=browser:browser.permissions-hardware
serial.requestPort();

function localWindow(window) {
    // @expect-no-error glass-lint rule=browser:browser.permissions-hardware
    window.navigator.usb.requestDevice({});
}
localWindow({ navigator: { usb: { requestDevice() {} } } });
