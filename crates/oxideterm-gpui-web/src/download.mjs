export function downloadFile(name, bytes) {
    downloadBlob(name, new Blob([bytes], {type: "application/octet-stream"}));
}

export function downloadBlob(name, blob) {
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    try {
        link.href = url;
        link.download = name;
        document.body.append(link);
        link.click();
    } finally {
        link.remove();
        // The browser consumes the URL after the click's default action.
        setTimeout(() => URL.revokeObjectURL(url), 0);
    }
}
