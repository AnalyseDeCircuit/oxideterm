export function chooseUploads(accept = "") {
    return new Promise((resolve, reject) => {
        const input = document.createElement("input");
        input.type = "file";
        input.multiple = true;
        input.accept = accept;
        input.addEventListener("cancel", () => resolve([]), {once: true});
        input.addEventListener("change", () => resolve(Array.from(input.files)), {once: true});
        try {
            if (input.showPicker) input.showPicker();
            else input.click();
        } catch (error) { reject(error); }
    });
}

export function readDirectoryBatch(reader) {
    return new Promise((resolve, reject) => reader.readEntries(resolve, reject));
}

export function entryFile(entry) {
    return new Promise((resolve, reject) => entry.file(resolve, reject));
}
