use super::{records::*, *};
use crate::AiImageAttachment;

pub(super) fn store(
    tx: &redb::WriteTransaction,
    conversation: &str,
    value: Value,
) -> Result<StoredValue> {
    let mut image: AiImageAttachment = serde_json::from_value(value)?;
    image.validate()?;
    uuid::Uuid::parse_str(&image.id)?;
    let key = format!("image:{}", image.id);
    if let Some(data) = image.data.take() {
        anyhow::ensure!(
            data.len() <= crate::AI_IMAGE_MAX_BYTES * 4 / 3 + 4,
            "Image attachment is too large"
        );
        let bytes = zeroize::Zeroizing::new(general_purpose::STANDARD.decode(data.as_bytes())?);
        anyhow::ensure!(
            bytes.len() == image.bytes,
            "Image attachment length changed"
        );
        let mut blobs = tx.open_table(BLOBS)?;
        let exists = {
            let existing = blobs.get((conversation, key.as_str()))?;
            if let Some(existing) = existing {
                anyhow::ensure!(
                    existing.value() == bytes.as_slice(),
                    "Image attachment identity changed"
                );
                true
            } else {
                false
            }
        };
        if !exists {
            blobs.insert((conversation, key.as_str()), bytes.as_slice())?;
        }
    } else {
        anyhow::ensure!(
            tx.open_table(BLOBS)?
                .get((conversation, key.as_str()))?
                .is_some(),
            "Image attachment is missing"
        );
    }
    let mut refs = tx.open_table(REFS)?;
    let count = refs
        .get((conversation, key.as_str()))?
        .map(|row| row.value())
        .unwrap_or(0);
    refs.insert((conversation, key.as_str()), count + 1)?;
    Ok(StoredValue::Image {
        key,
        metadata: serde_json::to_value(image)?,
    })
}

pub(super) fn hydrate(
    tx: &redb::ReadTransaction,
    conversation: &str,
    message: &mut AiChatMessage,
) -> Result<()> {
    if let Some(metadata) = &mut message.metadata {
        for image in &mut metadata.images {
            let table = tx.open_table(BLOBS)?;
            let key = format!("image:{}", image.id);
            let row = table
                .get((conversation, key.as_str()))?
                .ok_or_else(|| anyhow!("Image attachment is missing"))?;
            image.data = Some(Arc::from(general_purpose::STANDARD.encode(row.value())));
        }
    }
    Ok(())
}

impl ConversationStore {
    pub(super) fn hydrate_images(
        &self,
        conversation: &str,
        message: &mut AiChatMessage,
    ) -> Result<()> {
        let guard = self.db.read();
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?
            .begin_read()?;
        hydrate(&tx, conversation, message)
    }

    pub fn image_bytes(&self, conversation: &str, image_id: &str) -> Result<Arc<[u8]>> {
        uuid::Uuid::parse_str(image_id)?;
        let guard = self.db.read();
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?
            .begin_read()?;
        let table = tx.open_table(BLOBS)?;
        let key = format!("image:{image_id}");
        let row = table
            .get((conversation, key.as_str()))?
            .ok_or_else(|| anyhow!("Image attachment is missing"))?;
        Ok(Arc::from(row.value()))
    }
}
