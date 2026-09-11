use crate::{error::ErrorCode, protocol::LeaseView};
use std::time::{Duration, Instant};
use uuid::Uuid;
pub const LEASE_TTL: Duration = Duration::from_secs(15);
#[derive(Debug, Clone)]
struct Owner {
    client: Uuid,
    connection: Uuid,
    until: Instant,
}
#[derive(Debug, Default)]
pub struct Lease {
    epoch: u64,
    owner: Option<Owner>,
}
impl Lease {
    pub fn acquire(
        &mut self,
        client: Uuid,
        connection: Uuid,
        takeover: bool,
        now: Instant,
    ) -> Result<LeaseView, ErrorCode> {
        if let Some(owner) = self.owner.as_ref().filter(|o| o.until > now) {
            if owner.client == client && owner.connection == connection {
                self.owner.as_mut().unwrap().until = now + LEASE_TTL;
                return Ok(self.view(now).unwrap());
            }
            if !takeover {
                return Err(ErrorCode::LeaseBusy);
            }
        }
        self.epoch = self.epoch.checked_add(1).ok_or(ErrorCode::Internal)?;
        self.owner = Some(Owner {
            client,
            connection,
            until: now + LEASE_TTL,
        });
        Ok(self.view(now).unwrap())
    }
    pub fn verify(
        &self,
        client: Uuid,
        connection: Uuid,
        epoch: u64,
        now: Instant,
    ) -> Result<(), ErrorCode> {
        let o = self.owner.as_ref().ok_or(ErrorCode::LeaseRequired)?;
        if o.until <= now || o.client != client || o.connection != connection || self.epoch != epoch
        {
            return Err(ErrorCode::LeaseStale);
        }
        Ok(())
    }
    pub fn renew(
        &mut self,
        client: Uuid,
        connection: Uuid,
        epoch: u64,
        now: Instant,
    ) -> Result<LeaseView, ErrorCode> {
        self.verify(client, connection, epoch, now)?;
        self.owner.as_mut().unwrap().until = now + LEASE_TTL;
        Ok(self.view(now).unwrap())
    }
    pub fn release(
        &mut self,
        client: Uuid,
        connection: Uuid,
        epoch: u64,
        now: Instant,
    ) -> Result<(), ErrorCode> {
        self.verify(client, connection, epoch, now)?;
        self.owner = None;
        Ok(())
    }
    pub fn revoke_connection(&mut self, id: Uuid) {
        if self.owner.as_ref().is_some_and(|o| o.connection == id) {
            self.owner = None;
        }
    }
    pub fn revoke_client(&mut self, id: Uuid) {
        if self.owner.as_ref().is_some_and(|o| o.client == id) {
            self.owner = None;
        }
    }
    pub fn revoke_all(&mut self) {
        self.owner = None;
    }
    pub fn view(&self, now: Instant) -> Option<LeaseView> {
        self.owner
            .as_ref()
            .filter(|o| o.until > now)
            .map(|o| LeaseView {
                client_id: o.client,
                connection_id: o.connection,
                epoch: self.epoch,
                remaining_ms: o.until.duration_since(now).as_millis() as u64,
            })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_device_two_browser_tabs_are_different_writers() {
        let now = Instant::now();
        let c = Uuid::new_v4();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let mut l = Lease::default();
        let old = l.acquire(c, a, false, now).unwrap();
        assert_eq!(
            l.acquire(c, b, false, now).unwrap_err(),
            ErrorCode::LeaseBusy
        );
        let new = l.acquire(c, b, true, now).unwrap();
        assert!(new.epoch > old.epoch);
        assert!(l.verify(c, a, old.epoch, now).is_err());
    }
    #[test]
    fn renew_cannot_revive_expired_lease() {
        let mut l = Lease::default();
        let n = Instant::now();
        let c = Uuid::new_v4();
        let a = l.acquire(c, c, false, n).unwrap();
        assert!(l.renew(c, c, a.epoch, n + LEASE_TTL).is_err());
    }
    #[test]
    fn revoke_is_immediate() {
        let mut l = Lease::default();
        let n = Instant::now();
        let c = Uuid::new_v4();
        let a = l.acquire(c, c, false, n).unwrap();
        l.revoke_client(c);
        assert!(l.verify(c, c, a.epoch, n).is_err());
    }
}
