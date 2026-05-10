fn build_upload_readers(
    state: Arc<TrzszState>,
    owner_id: &str,
    paths: Vec<String>,
    policy: &TrzszTransferPolicy,
) -> Result<Vec<Box<dyn TrzszFileReader>>, TrzszError> {
    let entries = upload::build_upload_entries(
        &state,
        owner_id,
        TRZSZ_API_VERSION,
        paths,
        policy.allow_directory,
    )?;
    if !policy.allow_directory
        && entries
            .iter()
            .any(|entry| entry.is_dir || entry.rel_path.len() > 1)
    {
        return Err(TrzszError::DirectoryNotAllowed(
            "terminal settings".to_string(),
        ));
    }

    let file_count = entries.iter().filter(|entry| !entry.is_dir).count();
    if file_count > policy.max_file_count {
        return Err(TrzszError::MaxFileCountExceeded {
            selected: file_count,
            max: policy.max_file_count,
        });
    }

    let total_bytes = entries
        .iter()
        .filter(|entry| !entry.is_dir)
        .map(|entry| entry.size)
        .sum::<u64>();
    if total_bytes > policy.max_total_bytes {
        return Err(TrzszError::MaxTotalBytesExceeded {
            selected: total_bytes,
            max: policy.max_total_bytes,
        });
    }

    Ok(entries
        .into_iter()
        .map(|entry| {
            Box::new(NativeUploadReader::new(
                state.clone(),
                owner_id.to_string(),
                entry,
            )) as Box<dyn TrzszFileReader>
        })
        .collect())
}

struct NativeUploadReader {
    state: Arc<TrzszState>,
    owner_id: String,
    entry: TrzszUploadEntryDto,
    handle_id: Option<String>,
    offset: u64,
    closed: bool,
}

impl NativeUploadReader {
    fn new(state: Arc<TrzszState>, owner_id: String, entry: TrzszUploadEntryDto) -> Self {
        Self {
            state,
            owner_id,
            entry,
            handle_id: None,
            offset: 0,
            closed: false,
        }
    }

    fn ensure_handle(&mut self) -> Result<String, TrzszError> {
        if let Some(handle_id) = &self.handle_id {
            return Ok(handle_id.clone());
        }
        let handle = upload::open_upload_file(
            &self.state,
            &self.owner_id,
            TRZSZ_API_VERSION,
            self.entry.path.clone(),
        )?;
        self.handle_id = Some(handle.handle_id.clone());
        Ok(handle.handle_id)
    }
}

impl TrzszFileReader for NativeUploadReader {
    fn close_file(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        if let Some(handle_id) = self.handle_id.take() {
            let _ = upload::close_upload_file(
                &self.state,
                &self.owner_id,
                TRZSZ_API_VERSION,
                &handle_id,
            );
        }
    }

    fn path_id(&self) -> u64 {
        self.entry.path_id
    }

    fn rel_path(&self) -> &[String] {
        &self.entry.rel_path
    }

    fn is_dir(&self) -> bool {
        self.entry.is_dir
    }

    fn size(&self) -> u64 {
        self.entry.size
    }

    fn read_file(&mut self, max_len: usize) -> Result<Vec<u8>, TrzszError> {
        if self.closed || self.entry.is_dir {
            return Ok(Vec::new());
        }
        let handle_id = self.ensure_handle()?;
        let data = upload::read_upload_chunk(
            &self.state,
            &self.owner_id,
            TRZSZ_API_VERSION,
            &handle_id,
            self.offset,
            max_len,
        )?;
        self.offset = self.offset.saturating_add(data.len() as u64);
        Ok(data)
    }
}

impl Drop for NativeUploadReader {
    fn drop(&mut self) {
        self.close_file();
    }
}
