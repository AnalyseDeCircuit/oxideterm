// All paths are relative to the directory the user granted. Never resolve a browser path as a URL.
export function canPickDirectory() {
    return window.isSecureContext && typeof window.showDirectoryPicker === "function";
}

export function canMoveEntry(directory) {
    const api = directory ? globalThis.FileSystemDirectoryHandle : globalThis.FileSystemFileHandle;
    return typeof api?.prototype.move === "function";
}

export function sameDirectory(root, other) { return root.isSameEntry(other); }

export async function pickDirectory() {
    try {
        return await window.showDirectoryPicker({ mode: "readwrite" });
    } catch (error) {
        if (error.name === "AbortError") return null;
        throw error;
    }
}

function components(path) {
    if (path === "/") return [];
    const parts = path.slice(1).split("/");
    if (!path.startsWith("/") || parts.some(name => !name || name === "." || name === ".." || /[\\\0]/.test(name))) {
        throw new DOMException("Invalid folder path", "SecurityError");
    }
    return parts;
}

async function directory(root, parts) {
    let dir = root;
    for (const name of parts) dir = await dir.getDirectoryHandle(name);
    return dir;
}

export function openSubdirectory(root, path) { return directory(root, components(path)); }

async function parent(root, path) {
    const parts = components(path);
    const name = parts.pop();
    if (!name) throw new DOMException("Selected root", "InvalidModificationError");
    return [await directory(root, parts), name];
}

async function child(dir, name) {
    try {
        return await dir.getFileHandle(name);
    } catch (err) {
        if (err.name === "TypeMismatchError") return dir.getDirectoryHandle(name);
        if (err.name === "NotFoundError") return null;
        throw err;
    }
}

const conflict = () => new DOMException("File changed", "InvalidStateError");

export async function listDirectory(root, path) {
    const dir = await directory(root, components(path));
    const rows = [];
    for await (const handle of dir.values()) {
        if (handle.kind === "directory") {
            rows.push({ name: handle.name, directory: true });
        } else {
            const file = await handle.getFile();
            rows.push({ name: handle.name, directory: false, size: file.size, modified: file.lastModified });
        }
    }
    return JSON.stringify(rows);
}

export async function fileSnapshot(root, path) {
    const [dir, name] = await parent(root, path);
    return (await dir.getFileHandle(name)).getFile();
}

export async function startFile(root, path, size, modified, createNew) {
    const [dir, name] = await parent(root, path);
    let handle = await child(dir, name);
    if ((createNew && handle) || (!handle && modified !== undefined)) throw conflict();
    const fresh = !handle;
    handle ??= await dir.getFileHandle(name, { create: true });
    if (handle.kind !== "file") throw conflict();
    const writer = await handle.createWritable({ mode: "exclusive" });
    try {
        if (modified !== undefined) {
            const file = await handle.getFile();
            if (file.size !== size || file.lastModified !== modified) throw conflict();
        }
        return { writer, handle, dir, name, fresh, initial: fresh ? await handle.getFile() : null };
    } catch (err) {
        await writer.abort().catch(() => {});
        throw err;
    }
}

export function writeChunk(file, bytes) { return file.writer.write(bytes); }
export async function finishFile(file) {
    await file.writer.close();
    file.committed = true;
    return file.handle.getFile();
}
export async function abortFile(file) {
    await file.writer.abort().catch(() => {});
    if (file.fresh && !file.committed) {
        const current = await child(file.dir, file.name);
        if (current && await current.isSameEntry(file.handle)) {
            const data = await current.getFile();
            if (data.size === 0 && data.lastModified === file.initial.lastModified)
                await file.dir.removeEntry(file.name);
        }
    }
}

export async function saveFile(root, path, bytes, size, modified, createNew) {
    const file = await startFile(root, path, size, modified, createNew);
    try {
        await writeChunk(file, bytes);
        return await finishFile(file);
    } catch (err) {
        await abortFile(file).catch(() => {});
        throw err;
    }
}

export async function createDirectory(root, path) {
    const [dir, name] = await parent(root, path);
    if (await child(dir, name)) throw conflict();
    await dir.getDirectoryHandle(name, { create: true });
}

export async function removeEntry(root, path, recursive) {
    const [dir, name] = await parent(root, path);
    await dir.removeEntry(name, { recursive });
}

async function copyInto(source, dir, name) {
    if (await child(dir, name)) throw conflict();
    if (source.kind === "directory") {
        const dest = await dir.getDirectoryHandle(name, { create: true });
        for await (const entry of source.values()) await copyInto(entry, dest, entry.name);
    } else {
        const file = await source.getFile();
        const dest = await dir.getFileHandle(name, { create: true });
        const writer = await dest.createWritable({ mode: "exclusive" });
        await file.stream().pipeTo(writer);
    }
}

export async function moveOrCopy(root, from, to, move) {
    const [src, old] = await parent(root, from);
    const [dest, name] = await parent(root, to);
    const source = await child(src, old);
    if (!source) throw new DOMException("Missing source", "NotFoundError");
    if (source.kind === "directory" && await source.resolve(dest) !== null) throw conflict();
    if (move) {
        if (await child(dest, name)) throw conflict();
        if (typeof source.move !== "function") throw new DOMException("Rename unavailable", "NotSupportedError");
        await source.move(dest, name);
    } else {
        await copyInto(source, dest, name);
    }
}
