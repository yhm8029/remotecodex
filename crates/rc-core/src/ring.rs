use std::{collections::VecDeque,sync::Arc};
use crate::{frame::Frame,error::ErrorCode};
#[derive(Debug)]
pub struct EventRing { limit:usize,bytes:usize,events:VecDeque<Arc<Frame>> }
impl EventRing {
    pub fn new(limit:usize)->Self{Self{limit,bytes:0,events:VecDeque::new()}}
    pub fn push(&mut self,event:Frame)->Arc<Frame>{
        let e=Arc::new(event);self.bytes+=e.payload.len()+40;self.events.push_back(e.clone());
        while self.bytes>self.limit || self.events.len()>4096 {if let Some(old)=self.events.pop_front(){self.bytes-=old.payload.len()+40;}else{break;}}
        e
    }
    pub fn after(&self,seq:u64)->Result<Vec<Arc<Frame>>,ErrorCode>{
        if self.events.front().is_some_and(|f|seq.saturating_add(1)<f.sequence){return Err(ErrorCode::ResyncRequired);}
        Ok(self.events.iter().filter(|e|e.sequence>seq).cloned().collect())
    }
    pub fn bytes(&self)->usize{self.bytes}
}
#[cfg(test)]mod tests{use super::*;use uuid::Uuid;use bytes::Bytes;
    #[test]fn evicts_whole_events_not_ansi_fragments(){let mut r=EventRing::new(90);for sequence in 1..=10{r.push(Frame{kind:1,session_id:Uuid::nil(),generation:1,sequence,payload:Bytes::from_static(b"hello")});}assert!(r.bytes()<=90);assert!(r.after(0).is_err());assert_eq!(r.after(9).unwrap().len(),1);}
}
