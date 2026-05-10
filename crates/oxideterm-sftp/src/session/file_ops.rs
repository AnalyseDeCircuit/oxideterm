impl SftpSession {
    pub async fn delete(&self, path: &str) -> Result<(), SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        let metadata = self
            .sftp
            .symlink_metadata(&canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &canonical_path))?;
        if metadata.is_dir() && !metadata.is_symlink() {
            self.sftp
                .remove_dir(&canonical_path)
                .await
                .map_err(|error| self.map_sftp_error(error, &canonical_path))
        } else {
            self.sftp
                .remove_file(&canonical_path)
                .await
                .map_err(|error| self.map_sftp_error(error, &canonical_path))
        }
    }

    pub async fn delete_recursive(&self, path: &str) -> Result<u64, SftpError> {
        let canonical_path = self.resolve_path(path).await?;
        self.delete_recursive_inner(&canonical_path).await
    }

    pub async fn mkdir(&self, path: &str) -> Result<(), SftpError> {
        let canonical_path = if is_absolute_remote_path(path) {
            path.to_string()
        } else {
            join_remote_path(&self.cwd, path)
        };
        self.sftp
            .create_dir(&canonical_path)
            .await
            .map_err(|error| self.map_sftp_error(error, &canonical_path))
    }

    pub async fn rename(&self, old_path: &str, new_path: &str) -> Result<(), SftpError> {
        let old_canonical = self.resolve_path(old_path).await?;
        let new_canonical = if is_absolute_remote_path(new_path) {
            new_path.to_string()
        } else {
            let parent = old_canonical
                .rsplit_once('/')
                .map(|(parent, _)| parent)
                .filter(|parent| !parent.is_empty())
                .unwrap_or("/");
            join_remote_path(parent, new_path)
        };
        self.sftp
            .rename(&old_canonical, &new_canonical)
            .await
            .map_err(|error| self.map_sftp_error(error, &old_canonical))
    }

    async fn delete_recursive_inner(&self, path: &str) -> Result<u64, SftpError> {
        let metadata = self
            .sftp
            .symlink_metadata(path)
            .await
            .map_err(|error| self.map_sftp_error(error, path))?;
        if !metadata.is_dir() || metadata.is_symlink() {
            self.sftp
                .remove_file(path)
                .await
                .map_err(|error| self.map_sftp_error(error, path))?;
            return Ok(1);
        }

        let mut deleted_count = 0;
        let entries = self
            .list_dir(
                path,
                Some(ListFilter {
                    show_hidden: true,
                    pattern: None,
                    sort: SortOrder::Name,
                }),
            )
            .await?;
        for entry in entries {
            deleted_count += Box::pin(self.delete_recursive_inner(&entry.path)).await?;
        }
        self.sftp
            .remove_dir(path)
            .await
            .map_err(|error| self.map_sftp_error(error, path))?;
        Ok(deleted_count + 1)
    }
}
