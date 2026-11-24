// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use gpui::Context;
use oxideterm_sftp::bookmarks::{
    SftpPathBookmark, default_sftp_bookmark_name, new_sftp_path_bookmark_id,
};
#[cfg(not(target_family = "wasm"))]
use oxideterm_sftp::bookmarks::{load_sftp_path_bookmarks, save_sftp_path_bookmarks};
use std::io;
#[cfg(not(target_family = "wasm"))]
use std::path::PathBuf;

pub struct SftpBookmarks {
    #[cfg(not(target_family = "wasm"))]
    settings_path: PathBuf,
    entries: Option<Vec<SftpPathBookmark>>,
}

impl SftpBookmarks {
    #[cfg(not(target_family = "wasm"))]
    pub fn new(settings_path: PathBuf) -> Self {
        Self {
            settings_path,
            entries: None,
        }
    }

    /// Keep bookmarks for the lifetime of the browser workspace.
    #[cfg(target_family = "wasm")]
    pub fn in_memory() -> Self {
        Self { entries: None }
    }

    pub(crate) fn load(&mut self, cx: &mut Context<Self>) {
        if self.entries.is_none() {
            #[cfg(not(target_family = "wasm"))]
            let entries = load_sftp_path_bookmarks(&self.settings_path);
            #[cfg(target_family = "wasm")]
            let entries = Vec::new();
            self.entries = Some(entries);
            cx.notify();
        }
    }

    pub(crate) fn for_remote(&self, remote_key: &str) -> Vec<SftpPathBookmark> {
        let mut bookmarks: Vec<_> = self
            .entries
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|bookmark| bookmark.remote_key == remote_key)
            .cloned()
            .collect();
        bookmarks.sort_by_key(|bookmark| bookmark.created_at);
        bookmarks
    }

    pub fn save_paths(
        &mut self,
        remote_key: String,
        local_path: String,
        remote_path: String,
        name_template: &str,
        cx: &mut Context<Self>,
    ) -> io::Result<bool> {
        if local_path.is_empty() || remote_path.is_empty() {
            return Ok(false);
        }
        self.edit(cx, |bookmarks| {
            bookmarks.push(SftpPathBookmark {
                name: default_sftp_bookmark_name(bookmarks, &remote_key, name_template),
                id: new_sftp_path_bookmark_id(),
                local_path,
                remote_path,
                remote_key,
                created_at: oxideterm_local_files::now_ms(),
            });
            true
        })
    }

    pub fn remove(&mut self, id: &str, cx: &mut Context<Self>) -> io::Result<bool> {
        self.edit(cx, |bookmarks| {
            let len = bookmarks.len();
            bookmarks.retain(|bookmark| bookmark.id != id);
            bookmarks.len() != len
        })
    }

    pub fn rename(&mut self, id: &str, name: String, cx: &mut Context<Self>) -> io::Result<bool> {
        self.edit(cx, |bookmarks| {
            let Some(bookmark) = bookmarks.iter_mut().find(|bookmark| bookmark.id == id) else {
                return false;
            };
            bookmark.name = name;
            true
        })
    }

    fn edit(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut Vec<SftpPathBookmark>) -> bool,
    ) -> io::Result<bool> {
        self.load(cx);
        let entries = self.entries.as_mut().expect("bookmarks have been loaded");
        if !edit(entries) {
            return Ok(false);
        }
        cx.notify();
        #[cfg(not(target_family = "wasm"))]
        save_sftp_path_bookmarks(&self.settings_path, entries)?;
        Ok(true)
    }
}
