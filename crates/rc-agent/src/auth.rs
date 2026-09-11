use crate::store::{Device, Store};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use parking_lot::Mutex;
use rand::{rngs::OsRng, RngCore};
use rc_core::{error::ErrorCode, protocol::Scope};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub fn secret() -> String {
    let mut b = [0u8; 32];
    OsRng.fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}
fn key(s: &str) -> [u8; 32] {
    Sha256::digest(s.as_bytes()).into()
}
fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
#[derive(Clone)]
pub struct Principal {
    pub client_id: Uuid,
    pub scopes: Vec<Scope>,
    pub expires: Instant,
    pub revoked: CancellationToken,
}
impl Principal {
    pub fn require(&self, s: Scope) -> Result<(), ErrorCode> {
        if self.revoked.is_cancelled() {
            return Err(ErrorCode::DeviceRevoked);
        }
        if self.expires <= Instant::now() {
            return Err(ErrorCode::Unauthenticated);
        }
        if !self.scopes.contains(&s) {
            return Err(ErrorCode::Forbidden);
        }
        Ok(())
    }
}
struct PairTicket {
    scopes: Vec<Scope>,
    until: Instant,
}
struct Challenge {
    client: Uuid,
    message: String,
    until: Instant,
}
struct Access {
    client: Uuid,
    until: Instant,
}
struct WsTicket {
    principal: Principal,
    channel: String,
    session: Option<Uuid>,
    until: Instant,
}
struct DeviceState {
    device: Device,
    cancel: CancellationToken,
}
struct Inner {
    devices: HashMap<Uuid, DeviceState>,
    pairs: HashMap<[u8; 32], PairTicket>,
    challenges: HashMap<String, Challenge>,
    access: HashMap<[u8; 32], Access>,
    ws: HashMap<[u8; 32], WsTicket>,
    window_start: Instant,
    requests: u32,
}
pub struct Auth {
    blocked: std::sync::atomic::AtomicBool,
    inner: Mutex<Inner>,
    store: Arc<Store>,
    pub audience: String,
}
impl Auth {
    pub fn new(store: Arc<Store>, audience: String) -> anyhow::Result<Self> {
        let devices = store
            .devices()?
            .into_iter()
            .map(|d| {
                let cancel = CancellationToken::new();
                if d.revoked {
                    cancel.cancel();
                }
                (d.client_id, DeviceState { device: d, cancel })
            })
            .collect();
        Ok(Self {
            blocked: std::sync::atomic::AtomicBool::new(store.remote_blocked()?),
            store,
            audience,
            inner: Mutex::new(Inner {
                devices,
                pairs: HashMap::new(),
                challenges: HashMap::new(),
                access: HashMap::new(),
                ws: HashMap::new(),
                window_start: Instant::now(),
                requests: 0,
            }),
        })
    }
    fn prune(i: &mut Inner) {
        let now = Instant::now();
        i.pairs.retain(|_, v| v.until > now);
        i.challenges.retain(|_, v| v.until > now);
        i.access.retain(|_, v| v.until > now);
        i.ws.retain(|_, v| v.until > now);
    }
    fn anonymous_limit(i: &mut Inner) -> Result<(), ErrorCode> {
        let now = Instant::now();
        if now.duration_since(i.window_start) >= Duration::from_secs(60) {
            i.window_start = now;
            i.requests = 0;
        }
        i.requests += 1;
        if i.requests > 120 {
            return Err(ErrorCode::RateLimited);
        }
        Self::prune(i);
        Ok(())
    }
    fn ensure_open(&self) -> Result<(), ErrorCode> {
        if self.is_blocked() {
            Err(ErrorCode::Forbidden)
        } else {
            Ok(())
        }
    }
    pub fn is_blocked(&self) -> bool {
        self.blocked.load(std::sync::atomic::Ordering::Acquire)
    }
    pub fn resume_local(&self) {
        if self.store.set_remote_blocked(false).is_err() {
            tracing::error!("Could not persist local resume; remaining blocked");
            return;
        }
        {
            let mut i = self.inner.lock();
            for d in i.devices.values_mut() {
                if !d.device.revoked {
                    d.cancel = CancellationToken::new();
                }
            }
        }
        self.blocked
            .store(false, std::sync::atomic::Ordering::Release);
    }
    pub fn has_owner(&self) -> bool {
        self.inner
            .lock()
            .devices
            .values()
            .any(|d| !d.device.revoked && d.device.scopes.contains(&Scope::AdminDevices))
    }
    /// Called only by the same-user local pipe or a scoped, authenticated owner API.
    pub fn issue_pair_ticket(&self, scopes: Vec<Scope>) -> Result<String, ErrorCode> {
        self.ensure_open()?;
        if scopes.is_empty() {
            return Err(ErrorCode::InvalidRequest);
        }
        let mut i = self.inner.lock();
        Self::prune(&mut i);
        if i.pairs.len() >= 16 {
            return Err(ErrorCode::RateLimited);
        }
        let token = secret();
        i.pairs.insert(
            key(&token),
            PairTicket {
                scopes,
                until: Instant::now() + Duration::from_secs(300),
            },
        );
        Ok(token)
    }
    pub fn pair(&self, ticket: &str, public_key: &str, label: &str) -> Result<Device, ErrorCode> {
        self.ensure_open()?;
        if ticket.len() > 128
            || label.trim().is_empty()
            || label.len() > 160
            || public_key.len() > 256
        {
            return Err(ErrorCode::InvalidRequest);
        }
        let raw = URL_SAFE_NO_PAD
            .decode(public_key)
            .map_err(|_| ErrorCode::InvalidRequest)?;
        VerifyingKey::from_sec1_bytes(&raw).map_err(|_| ErrorCode::InvalidRequest)?;
        let mut i = self.inner.lock();
        Self::anonymous_limit(&mut i)?;
        let p = i
            .pairs
            .remove(&key(ticket))
            .ok_or(ErrorCode::Unauthenticated)?;
        if i.devices.values().filter(|d| !d.device.revoked).count() >= 64 {
            return Err(ErrorCode::RateLimited);
        }
        let d = Device {
            client_id: Uuid::new_v4(),
            public_key: raw,
            label: label.trim().into(),
            scopes: p.scopes,
            revoked: false,
        };
        self.store
            .save_device(&d)
            .map_err(|_| ErrorCode::Internal)?;
        i.devices.insert(
            d.client_id,
            DeviceState {
                device: d.clone(),
                cancel: CancellationToken::new(),
            },
        );
        Ok(d)
    }
    pub fn challenge(&self, client: Uuid) -> Result<(String, String), ErrorCode> {
        self.ensure_open()?;
        let mut i = self.inner.lock();
        Self::anonymous_limit(&mut i)?;
        if !i.devices.get(&client).is_some_and(|d| !d.device.revoked) {
            return Err(ErrorCode::Unauthenticated);
        }
        if i.challenges.len() >= 128 {
            return Err(ErrorCode::RateLimited);
        }
        let id = secret();
        let nonce = secret();
        let expiry = unix_ms() + 30_000;
        let message = format!(
            "RemoteCodex/v1\n{}\n{}\n{}\n{}",
            self.audience, client, nonce, expiry
        );
        i.challenges.insert(
            id.clone(),
            Challenge {
                client,
                message: message.clone(),
                until: Instant::now() + Duration::from_secs(30),
            },
        );
        Ok((id, message))
    }
    pub fn verify(&self, id: &str, sig: &str) -> Result<String, ErrorCode> {
        self.ensure_open()?;
        if id.len() > 128 || sig.len() > 256 {
            return Err(ErrorCode::InvalidRequest);
        }
        let mut i = self.inner.lock();
        Self::anonymous_limit(&mut i)?;
        let challenge = i.challenges.remove(id).ok_or(ErrorCode::Unauthenticated)?;
        let device = i
            .devices
            .get(&challenge.client)
            .filter(|d| !d.device.revoked)
            .ok_or(ErrorCode::Unauthenticated)?;
        let verifier = VerifyingKey::from_sec1_bytes(&device.device.public_key)
            .map_err(|_| ErrorCode::Unauthenticated)?;
        let bytes = URL_SAFE_NO_PAD
            .decode(sig)
            .map_err(|_| ErrorCode::Unauthenticated)?;
        let signature = Signature::from_slice(&bytes).map_err(|_| ErrorCode::Unauthenticated)?;
        verifier
            .verify(challenge.message.as_bytes(), &signature)
            .map_err(|_| ErrorCode::Unauthenticated)?;
        if i.access.len() >= 256 {
            return Err(ErrorCode::RateLimited);
        }
        let token = secret();
        i.access.insert(
            key(&token),
            Access {
                client: challenge.client,
                until: Instant::now() + Duration::from_secs(600),
            },
        );
        Ok(token)
    }
    pub fn authorize(&self, token: &str) -> Result<Principal, ErrorCode> {
        self.ensure_open()?;
        if token.len() > 128 {
            return Err(ErrorCode::Unauthenticated);
        }
        let i = self.inner.lock();
        let a = i
            .access
            .get(&key(token))
            .filter(|a| a.until > Instant::now())
            .ok_or(ErrorCode::Unauthenticated)?;
        let d = i
            .devices
            .get(&a.client)
            .filter(|d| !d.device.revoked)
            .ok_or(ErrorCode::DeviceRevoked)?;
        Ok(Principal {
            client_id: a.client,
            scopes: d.device.scopes.clone(),
            expires: a.until,
            revoked: d.cancel.clone(),
        })
    }
    pub fn device_principal(&self, client: Uuid) -> Result<Principal, ErrorCode> {
        self.ensure_open()?;
        let i = self.inner.lock();
        let d = i
            .devices
            .get(&client)
            .filter(|d| !d.device.revoked)
            .ok_or(ErrorCode::DeviceRevoked)?;
        Ok(Principal {
            client_id: client,
            scopes: d.device.scopes.clone(),
            expires: Instant::now() + Duration::from_secs(60),
            revoked: d.cancel.clone(),
        })
    }
    pub fn issue_ws(
        &self,
        p: Principal,
        channel: String,
        session: Option<Uuid>,
    ) -> Result<String, ErrorCode> {
        if !matches!(channel.as_str(), "control" | "terminal" | "media") {
            return Err(ErrorCode::InvalidRequest);
        }
        p.require(Scope::TerminalRead)?;
        if matches!(channel.as_str(), "terminal" | "media") && session.is_none() {
            return Err(ErrorCode::InvalidRequest);
        }
        let mut i = self.inner.lock();
        Self::prune(&mut i);
        if i.ws.len() >= 128 {
            return Err(ErrorCode::RateLimited);
        }
        let token = secret();
        i.ws.insert(
            key(&token),
            WsTicket {
                principal: p,
                channel,
                session,
                until: Instant::now() + Duration::from_secs(30),
            },
        );
        Ok(token)
    }
    pub fn consume_ws(
        &self,
        token: &str,
        channel: &str,
    ) -> Result<(Principal, Option<Uuid>), ErrorCode> {
        self.ensure_open()?;
        if token.len() > 128 {
            return Err(ErrorCode::Unauthenticated);
        }
        let mut i = self.inner.lock();
        Self::prune(&mut i);
        let t = i.ws.remove(&key(token)).ok_or(ErrorCode::Unauthenticated)?;
        if t.channel != channel {
            return Err(ErrorCode::Forbidden);
        }
        t.principal.require(Scope::TerminalRead)?;
        Ok((t.principal, t.session))
    }
    pub fn revoke(&self, id: Uuid) -> Result<(), ErrorCode> {
        let mut i = self.inner.lock();
        let d = i.devices.get_mut(&id).ok_or(ErrorCode::InvalidRequest)?;
        d.device.revoked = true;
        d.cancel.cancel();
        self.store
            .save_device(&d.device)
            .map_err(|_| ErrorCode::Internal)?;
        i.access.retain(|_, a| a.client != id);
        i.ws.retain(|_, a| a.principal.client_id != id);
        Ok(())
    }
    pub fn block_all(&self) {
        self.blocked
            .store(true, std::sync::atomic::Ordering::Release);
        if self.store.set_remote_blocked(true).is_err() {
            tracing::error!("Could not persist remote block; blocked until process restart");
        }
        let mut i = self.inner.lock();
        for d in i.devices.values_mut() {
            d.cancel.cancel();
        }
        i.access.clear();
        i.ws.clear();
        i.pairs.clear();
        i.challenges.clear();
    }
    pub fn devices_public(&self) -> Vec<serde_json::Value> {
        self.inner.lock().devices.values().map(|d|serde_json::json!({"client_id":d.device.client_id,"label":d.device.label,"scopes":d.device.scopes,"revoked":d.device.revoked,"fingerprint":URL_SAFE_NO_PAD.encode(Sha256::digest(&d.device.public_key))})).collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::{signature::Signer, SigningKey};
    fn auth() -> (Auth, tempfile::TempDir) {
        let d = tempfile::tempdir().unwrap();
        let s = Arc::new(Store::open(&d.path().join("test.db")).unwrap());
        (Auth::new(s, "test-host".into()).unwrap(), d)
    }
    #[test]
    fn signed_pair_token_replay_and_revocation() {
        let (a, _dir) = auth();
        let signing = SigningKey::random(&mut OsRng);
        let raw = signing.verifying_key().to_encoded_point(false);
        let t = a.issue_pair_ticket(Scope::owner()).unwrap();
        let dev = a
            .pair(&t, &URL_SAFE_NO_PAD.encode(raw.as_bytes()), "home")
            .unwrap();
        assert!(a
            .pair(&t, &URL_SAFE_NO_PAD.encode(raw.as_bytes()), "other")
            .is_err());
        let (id, msg) = a.challenge(dev.client_id).unwrap();
        let sig: Signature = signing.sign(msg.as_bytes());
        let encoded = URL_SAFE_NO_PAD.encode(sig.to_bytes());
        let token = a.verify(&id, &encoded).unwrap();
        assert!(a.verify(&id, &encoded).is_err());
        let p = a.authorize(&token).unwrap();
        let w = a.issue_ws(p.clone(), "control".into(), None).unwrap();
        assert!(a.consume_ws(&w, "control").is_ok());
        assert!(a.consume_ws(&w, "control").is_err());
        a.revoke(dev.client_id).unwrap();
        assert!(a.authorize(&token).is_err());
        assert!(p.revoked.is_cancelled());
    }
    #[test]
    fn wrong_key_cannot_sign() {
        let (a, _dir) = auth();
        let k = SigningKey::random(&mut OsRng);
        let other = SigningKey::random(&mut OsRng);
        let t = a.issue_pair_ticket(vec![Scope::TerminalRead]).unwrap();
        let d = a
            .pair(
                &t,
                &URL_SAFE_NO_PAD.encode(k.verifying_key().to_encoded_point(false).as_bytes()),
                "reader",
            )
            .unwrap();
        let (id, msg) = a.challenge(d.client_id).unwrap();
        let sig: Signature = other.sign(msg.as_bytes());
        assert!(a
            .verify(&id, &URL_SAFE_NO_PAD.encode(sig.to_bytes()))
            .is_err());
    }
    #[test]
    fn pairing_allows_new_device_when_existing_are_revoked() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(Store::open(&dir.path().join("test.db")).unwrap());

        let key = SigningKey::random(&mut OsRng);
        let raw = key.verifying_key().to_encoded_point(false);

        let mut old_ids: Vec<Uuid> = Vec::with_capacity(64);
        for i in 0..64 {
            let id = Uuid::new_v4();
            let dev = Device {
                client_id: id,
                public_key: raw.as_bytes().to_vec(),
                label: format!("revoked-{i}"),
                scopes: Scope::owner(),
                revoked: true,
            };
            store.save_device(&dev).unwrap();
            old_ids.push(id);
        }

        let a = Auth::new(store.clone(), "test-host".into()).unwrap();
        let ticket = a.issue_pair_ticket(Scope::owner()).unwrap();
        let new_dev = a
            .pair(
                &ticket,
                &URL_SAFE_NO_PAD.encode(raw.as_bytes()),
                "replacement",
            )
            .expect("revoked records must not exhaust active device quota");
        assert!(!new_dev.revoked);
        assert!(a.challenge(old_ids[0]).is_err());

        let a2 = Auth::new(store, "test-host".into()).unwrap();
        assert!(a2.challenge(old_ids[0]).is_err());
    }
    #[test]
    fn test_pair_revoke_then_pair_succeeds() {
        let (a, _dir) = auth();
        let key = SigningKey::random(&mut OsRng);
        let encoded =
            URL_SAFE_NO_PAD.encode(key.verifying_key().to_encoded_point(false).as_bytes());

        let mut first_id = None;
        for _ in 0..64 {
            let ticket = a.issue_pair_ticket(Scope::owner()).unwrap();
            let device = a.pair(&ticket, &encoded, "label").unwrap();
            if first_id.is_none() {
                first_id = Some(device.client_id);
            }
            a.revoke(device.client_id).unwrap();
        }

        let revoked_id = first_id.unwrap();

        let ticket = a.issue_pair_ticket(Scope::owner()).unwrap();
        let device = a.pair(&ticket, &encoded, "label").unwrap();

        let err = a.challenge(revoked_id).unwrap_err();
        assert!(matches!(err, ErrorCode::Unauthenticated));

        let devices = a.devices_public();
        let mut revoked_count = 0;
        let mut active_count = 0;
        for v in devices {
            let revoked = v["revoked"].as_bool().unwrap();
            if revoked {
                revoked_count += 1;
            } else {
                active_count += 1;
            }
        }
        assert_eq!(revoked_count, 64);
        assert_eq!(active_count, 1);
    }

    #[test]
    fn test_pair_rate_limit() {
        let (a, _dir) = auth();
        let key = SigningKey::random(&mut OsRng);
        let encoded =
            URL_SAFE_NO_PAD.encode(key.verifying_key().to_encoded_point(false).as_bytes());

        for _ in 0..64 {
            let ticket = a.issue_pair_ticket(Scope::owner()).unwrap();
            a.pair(&ticket, &encoded, "label").unwrap();
        }

        let ticket = a.issue_pair_ticket(Scope::owner()).unwrap();
        let result = a.pair(&ticket, &encoded, "label");
        assert!(matches!(result, Err(ErrorCode::RateLimited)));
    }
}
