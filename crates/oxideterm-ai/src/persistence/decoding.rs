use super::*;
use serde::de::{DeserializeSeed, SeqAccess, Visitor};

impl ConversationStore {
    pub(super) fn apply_encoded(&self, reader: impl std::io::Read) -> Result<()> {
        let guard = self.db.read();
        let db = guard
            .as_ref()
            .ok_or_else(|| anyhow!("History database is unavailable"))?;
        let tx = db.begin_write()?;
        ApplyMutations {
            tx: &tx,
            cache: &self.cache,
        }
        .deserialize(&mut rmp_serde::Deserializer::new(reader))?;
        tx.commit()?;
        Ok(())
    }
}

struct ApplyMutations<'a> {
    tx: &'a redb::WriteTransaction,
    cache: &'a parking_lot::Mutex<super::cache::HistoryCache>,
}
impl<'de> DeserializeSeed<'de> for ApplyMutations<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> std::result::Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for ApplyMutations<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("history mutations")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> std::result::Result<(), A::Error> {
        let mut collect = HashSet::new();
        while let Some(mut mutation) = sequence.next_element::<HistoryMutation>()? {
            if let Some(conversation) = mutation.collection_owner() {
                collect.insert(conversation.to_owned());
            }
            mutation.sanitize();
            if let HistoryMutation::DeleteConversation {
                conversation_id, ..
            } = &mutation
            {
                self.cache.lock().clear_conversation(conversation_id);
            }
            mutations::apply_mutation(self.tx, mutation)
                .map_err(|_| serde::de::Error::custom("History transaction failed"))?;
        }
        for conversation in collect {
            super::reachability::collect(self.tx, &conversation)
                .map_err(|_| serde::de::Error::custom("History reference cleanup failed"))?;
        }
        Ok(())
    }
}
