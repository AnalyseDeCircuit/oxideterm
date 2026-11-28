// Only profile metadata belongs here. Device grants and live sessions belong to the browser.
let database;
function openCatalog() {
    if (!database) {
        database = new Promise((resolve, reject) => {
            const request = indexedDB.open("oxideterm.connections", 1);
            request.onupgradeneeded = () => request.result.createObjectStore("catalogs");
            request.onblocked = () => reject(new DOMException("Catalog upgrade blocked", "InvalidStateError"));
            request.onerror = () => reject(request.error);
            request.onsuccess = () => {
                const db = request.result;
                db.onversionchange = () => { db.close(); database = undefined; };
                resolve(db);
            };
        }).catch(error => { database = undefined; throw error; });
    }
    return database;
}

export async function readCatalog(scope, defaults) {
    const db = await openCatalog();
    return new Promise((resolve, reject) => {
        const tx = db.transaction("catalogs", "readwrite");
        const records = tx.objectStore("catalogs");
        const request = records.get(scope);
        let catalog;
        request.onsuccess = () => {
            catalog = request.result;
            if (catalog === undefined) {
                catalog = defaults;
                records.add(catalog, scope);
            }
        };
        tx.oncomplete = () => resolve(catalog);
        tx.onabort = () => reject(tx.error ?? new DOMException("Catalog read aborted", "AbortError"));
    });
}

export async function writeCatalog(scope, previous, proposed) {
    const db = await openCatalog();
    const base = JSON.parse(previous);
    const next = JSON.parse(proposed);
    const equal = (a, b) => JSON.stringify(a) === JSON.stringify(b);
    return new Promise((resolve, reject) => {
        const tx = db.transaction("catalogs", "readwrite");
        const records = tx.objectStore("catalogs");
        const request = records.get(scope);
        let saved = false;
        tx.oncomplete = () => resolve(saved);
        tx.onabort = () => reject(tx.error ?? new DOMException("Catalog write aborted", "AbortError"));
        request.onsuccess = () => {
            try {
                // Renaming a folder also moves its descendants. Reject a stale tree edit as a unit.
                if (request.result === undefined) return;
                const current = JSON.parse(request.result);
                const editedGroups = !equal(next.groups, base.groups);
                const editedProfiles = !equal(next.profiles, base.profiles);
                if ((editedGroups || editedProfiles) && !equal(current.groups, base.groups)) return;
                if (editedGroups && !equal(current.profiles, base.profiles)) return;
                const profiles = new Map(Object.entries(current.profiles));
                for (const id of new Set([...Object.keys(base.profiles), ...Object.keys(next.profiles)])) {
                    const old = Object.hasOwn(base.profiles, id) ? base.profiles[id] : undefined;
                    const value = Object.hasOwn(next.profiles, id) ? next.profiles[id] : undefined;
                    if (equal(old, value)) continue;
                    if (!equal(profiles.get(id), old) && !equal(profiles.get(id), value)) return;
                    if (value === undefined) profiles.delete(id);
                    else profiles.set(id, value);
                }
                const byId = ([a], [b]) => a < b ? -1 : a > b ? 1 : 0;
                // Usage is independent of a profile edit. Keep the newest observed time across tabs.
                const recent = new Map(Object.entries(current.recent ?? {}));
                for (const [id, time] of Object.entries(next.recent ?? {})) {
                    if (time !== base.recent?.[id] && time > (recent.get(id) ?? "")) recent.set(id, time);
                }
                for (const id of recent.keys()) if (!profiles.has(id)) recent.delete(id);
                const merged = { groups: editedGroups ? next.groups : current.groups, profiles: Object.fromEntries([...profiles].sort(byId)) };
                if (recent.size) merged.recent = Object.fromEntries(recent);
                records.put(JSON.stringify(merged), scope);
                saved = true;
            } catch (error) {
                tx.abort();
                reject(error);
            }
        };
    });
}
