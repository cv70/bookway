use super::*;

#[derive(Default)]
pub(crate) struct MemoryEventDao {
    events: Mutex<HashMap<String, (String, pb::Event)>>,
}

#[async_trait]
impl EventDao for MemoryEventDao {
    async fn store(&self, events: Vec<AcceptedEvent>) -> Result<StoreResult, DaoError> {
        let mut stored_events = self.events.lock().await;
        let mut result = StoreResult::default();
        for accepted_event in events {
            if stored_events.contains_key(&accepted_event.event.event_id) {
                result.duplicate += 1;
                continue;
            }
            stored_events.insert(
                accepted_event.event.event_id.clone(),
                (accepted_event.user_id, accepted_event.event),
            );
            result.accepted += 1;
        }
        Ok(result)
    }
}
