import assert from "node:assert/strict";
import test from "node:test";
import {chooseDevice, inspectServices, retainGatt, subscribeRx} from "../src/ble/discovery.mjs";

test("request the picker in the gesture; no selection, permission errors and a different saved device stay distinct", async () => {
    const calls = [];
    const device = {id: "device-A", gatt: {}};
    const api = {requestDevice(options) { calls.push(options); return Promise.resolve(device); }};
    const pending = chooseDevice(api, ["custom-service"], "device-A");
    assert.deepEqual(calls, [{acceptAllDevices: true, optionalServices: ["custom-service"]}]);
    assert.equal(await pending, device);
    await assert.rejects(chooseDevice(api, [], "device-B"), e => e === "wrong-device");
    await assert.rejects(chooseDevice(undefined, []), e => e === "unavailable");
    for (const name of ["NotFoundError", "SecurityError", "NotAllowedError"]) {
        const failed = chooseDevice({requestDevice() { throw new DOMException("", name); }}, []);
        if (name === "NotFoundError") assert.equal(await failed, null);
        else await assert.rejects(failed, e => e === "denied");
    }
});

test("service discovery reports actual capabilities and does not disconnect another consumer", async () => {
    const error = new DOMException("", "NetworkError");
    const service = {uuid: "uart", async getCharacteristics() {
        return [{properties: {writeWithoutResponse: true}}, {properties: {indicate: true}}];
    }};
    const gatt = {
        connected: false, opens: 0, closes: 0,
        async connect() { this.opens++; this.connected = true; return this; },
        disconnect() { this.closes++; this.connected = false; },
        async getPrimaryServices() { return [service]; },
    };
    const terminal = retainGatt(gatt);
    assert.deepEqual(await inspectServices({gatt}), [
        {uuid: "uart", write: false, writeWithoutResponse: true, notify: true},
    ]);
    assert.equal(gatt.opens, 1);
    assert.equal(gatt.closes, 0);
    terminal.release();
    assert.equal(gatt.closes, 1);
    service.getCharacteristics = async () => { throw error; };
    await assert.rejects(inspectServices({gatt}), e => e === error);
    assert.equal(gatt.closes, 2);
    gatt.getPrimaryServices = async () => { throw error; };
    await assert.rejects(inspectServices({gatt}), e => e === error);
    assert.equal(gatt.closes, 3);
});

test("closing during GATT connection and closing one of two RX consumers releases only its resources", async () => {
    let finish;
    const gatt = {connected: false, connect() {
        return new Promise(resolve => { finish = () => { this.connected = true; resolve(this); }; });
    }, disconnect() { this.connected = false; }};
    const connecting = retainGatt(gatt);
    await Promise.resolve();
    connecting.release();
    finish();
    await connecting.ready;
    assert.equal(gatt.connected, false);

    const rx = new EventTarget();
    const events = [];
    rx.startNotifications = async () => events.push("start");
    rx.stopNotifications = async () => events.push("stop");
    const first = subscribeRx(rx, () => events.push("first"));
    const second = subscribeRx(rx, () => events.push("second"));
    await Promise.all([first.ready, second.ready]);
    first.release();
    rx.dispatchEvent(new Event("characteristicvaluechanged"));
    assert.deepEqual(events, ["start", "second"]);
    second.release();
    await new Promise(resolve => setImmediate(resolve));
    assert.deepEqual(events, ["start", "second", "stop"]);
});
