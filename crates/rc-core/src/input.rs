use crate::{
    error::ErrorCode,
    protocol::{MAX_INPUT, MAX_JS_INTEGER, MAX_PASTE},
};
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;
/// Per-session replay guard. Scopes, epoch and lease are checked BEFORE this.
/// This is not an exactly-once transaction across OS writes or host crashes.
#[derive(Debug)]
pub struct InputLedger {
    highest: HashMap<(Uuid, u64), u64>,
    ids: VecDeque<Uuid>,
    outcomes: HashMap<Uuid, &'static str>,
    limit: usize,
}
impl Default for InputLedger {
    fn default() -> Self {
        Self {
            highest: HashMap::new(),
            ids: VecDeque::new(),
            outcomes: HashMap::new(),
            limit: 1024,
        }
    }
}
impl InputLedger {
    pub fn accept(
        &mut self,
        connection: Uuid,
        lease: u64,
        id: Uuid,
        seq: u64,
        bytes: usize,
    ) -> Result<(), ErrorCode> {
        self.accept_limit(connection, lease, id, seq, bytes, MAX_INPUT)
    }
    pub fn accept_paste(
        &mut self,
        connection: Uuid,
        lease: u64,
        id: Uuid,
        seq: u64,
        bytes: usize,
    ) -> Result<(), ErrorCode> {
        self.accept_limit(connection, lease, id, seq, bytes, MAX_PASTE)
    }
    fn accept_limit(
        &mut self,
        connection: Uuid,
        lease: u64,
        id: Uuid,
        seq: u64,
        bytes: usize,
        max: usize,
    ) -> Result<(), ErrorCode> {
        if bytes == 0 || bytes > max || seq == 0 || seq > MAX_JS_INTEGER {
            return Err(ErrorCode::FrameTooLarge);
        }
        if self.outcomes.contains_key(&id)
            || self
                .highest
                .get(&(connection, lease))
                .is_some_and(|s| seq <= *s)
        {
            return Err(ErrorCode::DuplicateInput);
        }
        // A session has one writer. Old lease sequences are not retained indefinitely.
        self.highest
            .retain(|(c, l), _| *c == connection && *l == lease);
        self.highest.insert((connection, lease), seq);
        self.ids.push_back(id);
        self.outcomes.insert(id, "accepted");
        while self.ids.len() > self.limit {
            if let Some(x) = self.ids.pop_front() {
                self.outcomes.remove(&x);
            }
        }
        Ok(())
    }
    pub fn finish(&mut self, id: Uuid, status: &'static str) {
        if let Some(s) = self.outcomes.get_mut(&id) {
            *s = status;
        }
    }
    pub fn status(&self, id: Uuid) -> Option<&'static str> {
        self.outcomes.get(&id).copied()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_duplicate_enter_after_ack_loss() {
        let mut l = InputLedger::default();
        let c = Uuid::new_v4();
        let id = Uuid::new_v4();
        assert!(l.accept(c, 1, id, 1, 1).is_ok());
        l.finish(id, "written");
        assert_eq!(l.accept(c, 1, id, 2, 1), Err(ErrorCode::DuplicateInput));
    }
    #[test]
    fn decreasing_sequence_rejected() {
        let mut l = InputLedger::default();
        let c = Uuid::new_v4();
        l.accept(c, 1, Uuid::new_v4(), 10, 4).unwrap();
        assert!(l.accept(c, 1, Uuid::new_v4(), 9, 4).is_err());
    }
}
