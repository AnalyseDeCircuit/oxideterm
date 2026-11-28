// Persist a directory handle only while the user has favorites in that directory.
// Reopening still requires the picker; we only compare identities, never request permission here.
function openStore() {
    return new Promise((resolve, reject) => {
        const request = indexedDB.open("oxideterm.file-manager", 1);
        request.onupgradeneeded = () => request.result.createObjectStore("favorites");
        request.onerror = () => reject(request.error);
        request.onsuccess = () => resolve(request.result);
    });
}

function snapshot(db) {
    return new Promise((resolve, reject) => {
        const request = db.transaction("favorites").objectStore("favorites").get("directories");
        request.onsuccess = () => resolve(request.result ?? { revision: 0, roots: [] });
        request.onerror = () => reject(request.error);
    });
}

async function findRoot(roots, selected) {
    for (let i = 0; i < roots.length; i++) {
        if (await selected.isSameEntry(roots[i].handle)) return i;
    }
    return -1;
}

export async function readFavorites(root) {
    const db = await openStore();
    try {
        const { roots } = await snapshot(db);
        const i = await findRoot(roots, root);
        return i < 0 ? "[]" : roots[i].json;
    } finally {
        db.close();
    }
}

// isSameEntry yields outside IndexedDB. Check the revision again inside the write transaction.
function commit(db, revision, roots) {
    return new Promise((resolve, reject) => {
        const tx = db.transaction("favorites", "readwrite");
        const store = tx.objectStore("favorites");
        const current = store.get("directories");
        let written = false;
        tx.oncomplete = () => resolve(written);
        tx.onabort = () => reject(tx.error ?? new DOMException("Write aborted", "AbortError"));
        current.onsuccess = () => {
            if ((current.result?.revision ?? 0) !== revision) return;
            try {
                store.put({ revision: revision + 1, roots }, "directories");
                written = true;
            } catch (error) {
                tx.abort();
                reject(error);
            }
        };
    });
}

export async function writeFavorites(root, expected, next) {
    const db = await openStore();
    try {
        for (;;) {
            const { revision, roots } = await snapshot(db);
            const i = await findRoot(roots, root);
            if ((i < 0 ? "[]" : roots[i].json) !== expected) return false;
            if (next === expected) return true;
            if (next === "[]") roots.splice(i, 1);
            else if (i < 0) roots.push({ handle: root, json: next });
            else roots[i].json = next;
            if (await commit(db, revision, roots)) return true;
        }
    } finally {
        db.close();
    }
}
