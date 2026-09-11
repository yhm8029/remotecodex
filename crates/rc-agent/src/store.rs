use std::path::Path;
use parking_lot::Mutex;
use rusqlite::{Connection,params};
use serde::{Serialize,Deserialize};
use uuid::Uuid;
use rc_core::protocol::{Scope,SessionInfo};
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Device {pub client_id:Uuid,pub public_key:Vec<u8>,pub label:String,pub scopes:Vec<Scope>,pub revoked:bool}
pub struct Store {conn:Mutex<Connection>}
impl Store {
    pub fn open(path:&Path)->anyhow::Result<Self>{
        let conn=Connection::open(path)?;conn.pragma_update(None,"journal_mode","WAL")?;
        conn.pragma_update(None,"foreign_keys","ON")?;conn.busy_timeout(std::time::Duration::from_secs(2))?;
        conn.execute_batch(include_str!("schema.sql"))?;
        conn.execute("UPDATE sessions SET lifecycle='lost' WHERE lifecycle IN ('starting','running','closing')",[])?;
        Ok(Self{conn:Mutex::new(conn)})
    }
    pub fn devices(&self)->anyhow::Result<Vec<Device>>{
        let c=self.conn.lock();let mut st=c.prepare("SELECT client_id,public_key,label,scopes_json,revoked FROM devices")?;
        let rows=st.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Vec<u8>>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,bool>(4)?)))?;
        let mut out=Vec::new();for r in rows{let(id,public_key,label,scopes,revoked)=r?;out.push(Device{client_id:id.parse()?,public_key,label,scopes:serde_json::from_str(&scopes)?,revoked});}Ok(out)
    }
    pub fn save_device(&self,d:&Device)->anyhow::Result<()>{
        self.conn.lock().execute("INSERT INTO devices(client_id,public_key,label,scopes_json,revoked) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(client_id) DO UPDATE SET scopes_json=excluded.scopes_json,revoked=excluded.revoked",params![d.client_id.to_string(),d.public_key,d.label,serde_json::to_string(&d.scopes)?,d.revoked])?;Ok(())
    }
    pub fn project_for_cwd(&self,cwd:&str,requested:Option<Uuid>)->anyhow::Result<Uuid>{
        use rusqlite::OptionalExtension;let mut c=self.conn.lock();let tx=c.transaction()?;
        let existing:Option<String>=tx.query_row("SELECT project_id FROM projects WHERE canonical_cwd=?1 COLLATE NOCASE",[cwd],|r|r.get(0)).optional()?;
        let id=if let Some(existing)=existing{let id:Uuid=existing.parse()?;if requested.is_some_and(|r|r!=id){anyhow::bail!("Project does not match canonical folder");}id}
        else{let id=requested.unwrap_or_else(Uuid::new_v4);tx.execute("INSERT INTO projects(project_id,canonical_cwd) VALUES(?1,?2)",params![id.to_string(),cwd])?;id};tx.commit()?;Ok(id)
    }
    pub fn save_session(&self,s:&SessionInfo)->anyhow::Result<()>{
        let state=serde_json::to_value(&s.state)?.as_str().unwrap_or("lost").to_owned();
        self.conn.lock().execute("INSERT INTO sessions(session_id,metadata_json,lifecycle) VALUES(?1,?2,?3) ON CONFLICT(session_id) DO UPDATE SET metadata_json=excluded.metadata_json,lifecycle=excluded.lifecycle",params![s.session_id.to_string(),serde_json::to_string(s)?,state])?;Ok(())
    }
    pub fn history(&self)->anyhow::Result<Vec<serde_json::Value>>{
        let c=self.conn.lock();let mut st=c.prepare("SELECT metadata_json,lifecycle FROM sessions ORDER BY created_at DESC LIMIT 100")?;
        let rows=st.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?;
        let mut out=Vec::new();for row in rows{let(m,l)=row?;let mut v:serde_json::Value=serde_json::from_str(&m)?;v["state"]=l.into();out.push(v);}Ok(out)
    }
    pub fn remote_blocked(&self)->anyhow::Result<bool>{
        use rusqlite::OptionalExtension;
        let value:Option<String>=self.conn.lock().query_row("SELECT value FROM settings WHERE key='remote_blocked'",[],|r|r.get(0)).optional()?;Ok(value.as_deref()==Some("true"))
    }
    pub fn set_remote_blocked(&self,value:bool)->anyhow::Result<()>{self.conn.lock().execute("INSERT INTO settings(key,value) VALUES('remote_blocked',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[if value{"true"}else{"false"}])?;Ok(())}
    pub fn reserve_preview_slot(&self,origin:&str,project:Uuid)->anyhow::Result<()>{
        use rusqlite::OptionalExtension;let mut c=self.conn.lock();let tx=c.transaction()?;
        let old:Option<String>=tx.query_row("SELECT project_id FROM preview_slots WHERE origin=?1",[origin],|r|r.get(0)).optional()?;
        if let Some(old)=old{if old!=project.to_string(){anyhow::bail!("Origin remains bound to its original project; do not reuse browser state");}}
        else{tx.execute("INSERT INTO preview_slots(origin,project_id) VALUES(?1,?2)",params![origin,project.to_string()])?;}tx.commit()?;Ok(())
    }
    pub fn audit(&self,client:Option<Uuid>,action:&str,resource:Option<Uuid>)->anyhow::Result<()>{
        // Metadata only: never raw keystrokes, terminal payloads, bearer tokens, or pairing tickets.
        let c=self.conn.lock();c.execute("INSERT INTO audit_events(client_id,action,resource_id) VALUES(?1,?2,?3)",params![client.map(|x|x.to_string()),action,resource.map(|x|x.to_string())])?;
        c.execute("DELETE FROM audit_events WHERE at < datetime('now','-7 days') OR event_id < (SELECT COALESCE(MAX(event_id),0)-10000 FROM audit_events)",[])?;Ok(())
    }
}
