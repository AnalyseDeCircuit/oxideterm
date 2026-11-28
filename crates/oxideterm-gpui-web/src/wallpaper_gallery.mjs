// Blobs live in the site's gallery. Neither the picker nor a reload grants filesystem access.
function openGallery() {
    return new Promise((resolve, reject) => {
        const request = indexedDB.open("oxideterm.backgrounds", 1);
        request.onupgradeneeded = () => {
            request.result.createObjectStore("thumbnails", { keyPath: "id" });
            request.result.createObjectStore("images");
        };
        request.onerror = () => reject(request.error);
        request.onblocked = () => reject(new Error("storage"));
        request.onsuccess = () => resolve(request.result);
    });
}

async function transaction(mode, work) {
    const db = await openGallery();
    try {
        return await new Promise((resolve,reject) => {
            const tx = db.transaction(["thumbnails","images"],mode);
            const result = work(tx.objectStore("thumbnails"),tx.objectStore("images"));
            tx.oncomplete = () => resolve(result?.result);
            tx.onabort = () => reject(tx.error ?? new Error("storage"));
        });
    } finally { db.close(); }
}

export function listWallpapers() {
    return transaction("readonly", thumbs => thumbs.getAll()).then(rows => rows.sort((a, b) => a.added - b.added));
}

export async function loadWallpaper(id) {
    const blob = await transaction("readonly", (_, images) => images.get(id));
    if (!blob) throw "missing";
    return { type: blob.type, bytes: new Uint8Array(await blob.arrayBuffer()) };
}

export function removeWallpapers(ids) {
    return transaction("readwrite", (thumbs, images) => {
        for (const id of ids) {
            thumbs.delete(id);
            images.delete(id);
        }
    });
}

function format(bytes) {
    const word = offset => String.fromCharCode(...bytes.subarray(offset, offset + 4));
    if (bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff) return ["image/jpeg", false];
    if (word(0) === "RIFF" && word(8) === "WEBP") return ["image/webp", word(12) === "VP8X" && !!(bytes[20] & 2)];
    if (word(0) === "GIF8") return ["image/gif", true];
    if (bytes[0] === 137 && word(1) === "PNG\r") {
        const view = new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);
        for (let i=8;i+12<=bytes.length;i+=12+view.getUint32(i)) {
            if (word(i+4) === "acTL") throw "unsupported";
            if (word(i+4) === "IDAT") return ["image/png", false];
        }
    }
    throw "unsupported";
}

async function prepare(file, limit) {
    if (file.size > limit) throw "size";
    const bytes = new Uint8Array(await file.arrayBuffer());
    const [type, animated] = format(bytes);
    const blob = file.slice(0, file.size, type);
    let frames = 1;
    if (animated) {
        if (!globalThis.ImageDecoder || !(await ImageDecoder.isTypeSupported(type))) throw "unsupported";
        try {
            const decoder = new ImageDecoder({ type, data: bytes });
            try {
                await decoder.tracks.ready;
                frames = decoder.tracks.selectedTrack?.frameCount;
                if (!frames) throw "decode";
            } finally {
                decoder.close();
            }
        } catch { throw "decode"; }
    }
    const url = URL.createObjectURL(blob);
    try {
        const image = new Image();
        const loaded = new Promise((ok, no) => {
            image.onload = ok;
            image.onerror = () => no("decode");
        });
        image.src = url;
        await loaded;
        const width = image.naturalWidth, height = image.naturalHeight;
        if (!width || !height) throw "decode";
        if (width * height * 4 * frames > limit) throw "size";
        try { await image.decode(); } catch { throw "decode"; }
        const scale = Math.min(1, 256 / Math.max(width, height));
        const canvas = document.createElement("canvas");
        canvas.width = Math.max(1, Math.round(width * scale));
        canvas.height = Math.max(1, Math.round(height * scale));
        canvas.getContext("2d").drawImage(image, 0, 0, canvas.width, canvas.height);
        const thumb = await new Promise((ok, no) =>
            canvas.toBlob(b => b ? ok(b) : no("decode"), "image/png"));
        const id = crypto.randomUUID();
        return { thumb: { id, name: file.name, type, bytes: new Uint8Array(await thumb.arrayBuffer()) }, blob };
    } finally {
        URL.revokeObjectURL(url);
    }
}

export async function importWallpapers(files, limit) {
    const selected = [];
    const addedAt = Date.now();
    for (const file of files) {
        const item = await prepare(file, limit);
        item.thumb.added = addedAt + selected.length / files.length;
        selected.push(item);
    }
    if (selected.length) await transaction("readwrite",(thumbs,images)=>{
        for (const {thumb,blob} of selected) {
            thumbs.add(thumb);
            images.add(blob, thumb.id);
        }
    });
    return selected.map(({thumb})=>thumb);
}
