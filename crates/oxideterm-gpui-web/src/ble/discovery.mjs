// requestDevice must run in the click handler, before GPUI schedules the continuation.
export async function chooseDevice(bluetooth, optionalServices, expectedId) {
    if (!bluetooth?.requestDevice) throw "unavailable";
    let device;
    try {
        device = await bluetooth.requestDevice({acceptAllDevices: true, optionalServices});
    } catch (error) {
        if (error?.name === "NotFoundError") return null;
        if (error?.name === "NotAllowedError" || error?.name === "SecurityError") throw "denied";
        throw "scan";
    }
    if (!device.id || !device.gatt) throw "inspect";
    if (expectedId != null && device.id !== expectedId) throw "wrong-device";
    return device;
}

// A service probe and open terminals may use the same browser GATT connection.
// Acquire synchronously so cancellation can release a still-pending connect().
export class BrowserBleLease {
    constructor(ready, release) {
        this.ready = ready;
        this.cleanup = release;
    }
    release() {
        const cleanup = this.cleanup;
        this.cleanup = null;
        cleanup?.();
    }
}

const connections = new WeakMap();
export function retainGatt(gatt) {
    let connection = connections.get(gatt);
    if (!connection || (connection.connected && !gatt.connected)) {
        connection = {users: 0, connected: false};
        connections.set(gatt, connection);
        const current = connection;
        current.ready = Promise.resolve().then(() => {
            if (gatt.connected) return gatt;
            current.ownsConnection = true;
            return gatt.connect();
        })
            .then(server => {
                current.connected = true;
                disconnectIfUnused(gatt, current);
                return server;
            }, error => {
                if (connections.get(gatt) === current) connections.delete(gatt);
                throw error;
            });
    }
    connection.users++;
    return new BrowserBleLease(connection.ready, () => {
        connection.users--;
        disconnectIfUnused(gatt, connection);
    });
}

function disconnectIfUnused(gatt, entry) {
    if (entry.users || !entry.connected || connections.get(gatt) !== entry) return;
    connections.delete(gatt);
    if (entry.ownsConnection && gatt.connected) gatt.disconnect();
}

export async function inspectServices(device) {
    const lease = retainGatt(device.gatt);
    try {
        const server = await lease.ready;
        const services = [];
        for (const service of await server.getPrimaryServices()) {
            const chars = await service.getCharacteristics();
            services.push({
                uuid: service.uuid,
                write: chars.some(c => c.properties.write),
                writeWithoutResponse: chars.some(c => c.properties.writeWithoutResponse),
                notify: chars.some(c => c.properties.notify || c.properties.indicate),
            });
        }
        if (!services.length) throw "inspect";
        return services;
    } finally {
        lease.release();
    }
}

// start/stopNotifications applies to the characteristic, not to one listener.
const notifications = new WeakMap();
export function subscribeRx(characteristic, callback) {
    let state = notifications.get(characteristic);
    if (!state) {
        state = {users: 0, enabled: false, queue: Promise.resolve()};
        notifications.set(characteristic, state);
    }
    function synchronize() {
        const operation = state.queue.then(async () => {
            if (state.users && !state.enabled) {
                await characteristic.startNotifications();
                state.enabled = true;
            } else if (!state.users && state.enabled) {
                try { await characteristic.stopNotifications(); }
                finally { state.enabled = false; }
            }
        });
        state.queue = operation.catch(() => {});
        return operation;
    }
    characteristic.addEventListener("characteristicvaluechanged", callback);
    state.users++;
    return new BrowserBleLease(synchronize(), () => {
        characteristic.removeEventListener("characteristicvaluechanged", callback);
        state.users--;
        synchronize();
    });
}
