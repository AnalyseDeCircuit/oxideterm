impl ConnectionStore {
    pub fn local_terminal_profiles(&self) -> &[LocalTerminalProfile] {
        &self.data.local_terminal_profiles
    }

    pub fn upsert_local_terminal_profile(
        &mut self,
        request: SaveLocalTerminalProfileRequest,
    ) -> Result<LocalTerminalProfile> {
        let name = request.name.trim().to_owned();
        if name.is_empty() {
            bail!("local terminal profile name is required");
        }
        let group = normalize_optional_group_name(request.group.as_deref())?;
        let now = Utc::now();
        let id = request.id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let previous = self.data.clone();
        let existing = self
            .data
            .local_terminal_profiles
            .iter()
            .find(|p| p.id == id);
        let profile = LocalTerminalProfile {
            id: id.clone(),
            name,
            group,
            icon: normalize_optional_text(request.icon),
            color: normalize_optional_text(request.color),
            icon_background_color: normalize_optional_text(request.icon_background_color),
            shell_id: normalize_optional_text(request.shell_id),
            cwd: normalize_optional_text(request.cwd),
            created_at: existing.map_or(now, |p| p.created_at),
            updated_at: now,
            last_used_at: existing.and_then(|p| p.last_used_at),
        };
        self.data.local_terminal_profiles.retain(|p| p.id != id);
        self.data.local_terminal_profiles.push(profile.clone());
        self.data.local_terminal_tombstones.retain(|p| p.id != id);
        if let Some(group) = &profile.group {
            if !self.data.groups.contains(group) {
                self.data.groups.push(group.clone());
            }
        }
        if let Err(error) = self.save() {
            self.data = previous;
            return Err(error);
        }
        Ok(profile)
    }

    pub fn delete_local_terminal_profile(&mut self, id: &str) -> Result<bool> {
        if !self.data.local_terminal_profiles.iter().any(|p| p.id == id) {
            return Ok(false);
        }
        let previous = self.data.clone();
        self.data.local_terminal_profiles.retain(|p| p.id != id);
        self.data.local_terminal_tombstones.retain(|p| p.id != id);
        self.data
            .local_terminal_tombstones
            .push(DeletedConnectionTombstone {
                id: id.to_owned(),
                deleted_at: Utc::now(),
            });
        if let Err(error) = self.save() {
            self.data = previous;
            return Err(error);
        }
        Ok(true)
    }

    pub fn mark_local_terminal_profile_used(&mut self, id: &str) -> Result<bool> {
        let Some(profile) = self
            .data
            .local_terminal_profiles
            .iter_mut()
            .find(|p| p.id == id)
        else {
            return Ok(false);
        };
        let previous = profile.last_used_at;
        profile.last_used_at = Some(Utc::now());
        // Usage belongs to this device and does not advance the synced configuration revision.
        if let Err(error) = self.save() {
            self.data
                .local_terminal_profiles
                .iter_mut()
                .find(|p| p.id == id)
                .unwrap()
                .last_used_at = previous;
            return Err(error);
        }
        Ok(true)
    }

    fn apply_local_terminal_profiles(
        &mut self,
        profiles: Vec<LocalTerminalProfile>,
        tombstones: Vec<DeletedConnectionTombstone>,
        strategy: SavedConnectionsConflictStrategy,
        result: &mut ApplySavedConnectionsSyncSnapshotResult,
    ) -> Result<()> {
        for tombstone in tombstones {
            if self
                .data
                .local_terminal_profiles
                .iter()
                .any(|p| p.id == tombstone.id && p.updated_at > tombstone.deleted_at)
            {
                result.conflicts += 1;
                result.skipped += 1;
                continue;
            }
            if self
                .data
                .local_terminal_tombstones
                .iter()
                .any(|t| t.id == tombstone.id && t.deleted_at >= tombstone.deleted_at)
            {
                continue;
            }
            self.data
                .local_terminal_profiles
                .retain(|p| p.id != tombstone.id);
            self.data
                .local_terminal_tombstones
                .retain(|t| t.id != tombstone.id);
            self.data.local_terminal_tombstones.push(tombstone);
            result.applied += 1;
        }
        for mut profile in profiles {
            if profile.id.trim().is_empty() || profile.name.trim().is_empty() {
                bail!("invalid local terminal profile");
            }
            profile.group = normalize_optional_group_name(profile.group.as_deref())?;
            if self
                .data
                .local_terminal_tombstones
                .iter()
                .any(|t| t.id == profile.id && t.deleted_at >= profile.updated_at)
            {
                result.conflicts += 1;
                result.skipped += 1;
                continue;
            }
            let existing = self
                .data
                .local_terminal_profiles
                .iter()
                .find(|p| p.id == profile.id);
            if existing.is_some_and(|p| {
                p.updated_at > profile.updated_at
                    && strategy != SavedConnectionsConflictStrategy::Replace
            }) {
                result.conflicts += 1;
                result.skipped += 1;
                continue;
            }
            if existing.is_none()
                && strategy == SavedConnectionsConflictStrategy::Skip
                && self
                    .data
                    .local_terminal_profiles
                    .iter()
                    .any(|p| p.name == profile.name)
            {
                result.conflicts += 1;
                result.skipped += 1;
                continue;
            }
            profile.last_used_at = existing.and_then(|p| p.last_used_at);
            self.data
                .local_terminal_tombstones
                .retain(|t| t.id != profile.id);
            self.data
                .local_terminal_profiles
                .retain(|p| p.id != profile.id);
            if let Some(group) = &profile.group {
                if !self.data.groups.contains(group) {
                    self.data.groups.push(group.clone());
                }
            }
            self.data.local_terminal_profiles.push(profile);
            result.applied += 1;
        }
        Ok(())
    }
}
