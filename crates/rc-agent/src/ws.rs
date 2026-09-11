use std::{sync::Arc,time::{Duration,Instant}};
use axum::{extract::{State,ws::{WebSocketUpgrade,WebSocket,Message}},http::HeaderMap,response::Response};
use futures_util::{SinkExt,StreamExt};
use tokio::sync::{mpsc,broadcast};
use uuid::Uuid;
use bytes::Bytes;
use base64::{Engine,engine::general_purpose::STANDARD};
use rc_core::{error::ErrorCode,protocol::*,frame::{self,Frame}};
use crate::{http::{Shared,Failure},auth::Principal};

pub(crate) fn ticket(h:&HeaderMap)->Result<String,Failure>{
    let raw=h.get("sec-websocket-protocol").and_then(|s|s.to_str().ok()).ok_or(ErrorCode::Unauthenticated)?;
    let parts=raw.split(',').map(str::trim).collect::<Vec<_>>();
    if !parts.contains(&"rctm.v1"){return Err(ErrorCode::InvalidRequest.into());}
    let candidates=parts.iter().filter_map(|p|p.strip_prefix("rc-ticket.")).collect::<Vec<_>>();
    if candidates.len()!=1{return Err(ErrorCode::Unauthenticated.into());}Ok(candidates[0].into())
}
pub async fn control_upgrade(State(s):State<Shared>,h:HeaderMap,upgrade:WebSocketUpgrade)->Result<Response,Failure>{if !h.get("origin").and_then(|v|v.to_str().ok()).is_some_and(|v|s.config.origin_allowed(v)){return Err(ErrorCode::OriginRejected.into());}let(p,_)=s.auth.consume_ws(&ticket(&h)?,"control")?;Ok(upgrade.protocols(["rctm.v1"]).max_message_size(32768).max_frame_size(32768).on_upgrade(move|socket|control(s,p,socket)))}
pub async fn terminal_upgrade(State(s):State<Shared>,h:HeaderMap,upgrade:WebSocketUpgrade)->Result<Response,Failure>{if !h.get("origin").and_then(|v|v.to_str().ok()).is_some_and(|v|s.config.origin_allowed(v)){return Err(ErrorCode::OriginRejected.into());}let(p,id)=s.auth.consume_ws(&ticket(&h)?,"terminal")?;let id=id.ok_or(ErrorCode::InvalidRequest)?;s.sessions.get(id)?;Ok(upgrade.protocols(["rctm.v1"]).max_message_size(32768).on_upgrade(move|socket|terminal(s,p,id,socket)))}
fn text(v:serde_json::Value)->Message{Message::Text(v.to_string().into())}

async fn control(state:Shared,mut p:Principal,socket:WebSocket){
    let connection=Uuid::new_v4();let(mut sink,mut stream)=socket.split();let mut events=state.sessions.events.subscribe();
    let(tx,mut rx)=mpsc::channel::<serde_json::Value>(128);let stop=tokio_util::sync::CancellationToken::new();
    let hello=serde_json::json!({"type":"hello","connection_id":connection,"client_id":p.client_id,"agent_epoch":state.sessions.epoch,"protocol":1,"lease_ttl_ms":15000});
    if sink.send(text(hello)).await.is_err(){return;}
    let expires=tokio::time::sleep_until(p.expires.into());tokio::pin!(expires);
    let(mut window,mut count)=(Instant::now(),0u32);let mut children=tokio::task::JoinSet::new();
    loop{
        tokio::select!{
            biased;
            _=p.revoked.cancelled()=>break,
            _=state.shutdown.cancelled()=>break,
            _=&mut expires=>break,
            Some(reply)=rx.recv()=>{if !matches!(tokio::time::timeout(Duration::from_secs(3),sink.send(text(reply))).await,Ok(Ok(()))){break;}},
            Some(result)=stream.next()=>{
                let Ok(message)=result else{break;};
                let Message::Text(raw)=message else{if matches!(message,Message::Close(_)){break;}continue;};
                if window.elapsed()>=Duration::from_secs(1){window=Instant::now();count=0;}count+=1;if count>2000{break;}
                let received=Instant::now();
                let msg=match serde_json::from_str::<ControlMessage>(&raw){Ok(m)=>m,Err(_)=>{let _=sink.send(text(serde_json::json!({"type":"error","code":"INVALID_REQUEST"}))).await;continue;}};
                if let ControlMessage::RefreshAuth{request_id,ticket}= &msg{
                    match state.auth.consume_ws(ticket,"control"){
                        Ok((next,None)) if next.client_id==p.client_id=>{p=next;expires.as_mut().reset(p.expires.into());let _=sink.send(text(serde_json::json!({"type":"auth_refreshed","request_id":request_id}))).await;},
                        _=>break,
                    }
                    continue;
                }
                let id=msg.request_id();
                let result=dispatch(&state,&p,connection,msg,&tx,&mut children,received).await;
                if let Err(code)=result{let _=sink.send(text(serde_json::json!({"type":"rejected","request_id":id,"code":code}))).await;}
            },
            event=events.recv()=>match event{Ok(v)=>{if sink.send(text(v)).await.is_err(){break;}},Err(broadcast::error::RecvError::Lagged(_))=>{let _=sink.send(text(serde_json::json!({"type":"sessions_resync"}))).await;},Err(_)=>break},
            Some(_)=children.join_next(),if !children.is_empty()=>{},
        }
    }
    stop.cancel();children.abort_all();state.sessions.revoke_connection(connection);let _=sink.close().await;
}
async fn dispatch(state:&Shared,p:&Principal,connection:Uuid,msg:ControlMessage,tx:&mpsc::Sender<serde_json::Value>,children:&mut tokio::task::JoinSet<()>,received:Instant)->Result<(),ErrorCode>{
    let request_id=msg.request_id();
    let reply=match msg{
        ControlMessage::RefreshAuth{..}=>return Err(ErrorCode::InvalidRequest),
        ControlMessage::Ping{..}=>serde_json::json!({"type":"pong","request_id":request_id}),
        ControlMessage::LeaseAcquire{session_id,takeover,..}=>{let lease=state.sessions.get(session_id)?.acquire(p,connection,takeover)?;serde_json::json!({"type":"lease","request_id":request_id,"session_id":session_id,"lease":lease})},
        ControlMessage::LeaseRenew{session_id,lease_epoch,..}=>{let lease=state.sessions.get(session_id)?.renew(p,connection,lease_epoch)?;serde_json::json!({"type":"lease","request_id":request_id,"session_id":session_id,"lease":lease})},
        ControlMessage::LeaseRelease{session_id,lease_epoch,..}=>{state.sessions.get(session_id)?.release(p,connection,lease_epoch)?;serde_json::json!({"type":"lease_released","request_id":request_id,"session_id":session_id})},
        ControlMessage::Resize{session_id,agent_epoch,generation,lease_epoch,cols,rows,..}=>{
            let s=state.sessions.get(session_id)?;s.validate_generation(agent_epoch,generation)?;let p=p.clone();
            tokio::task::spawn_blocking(move||s.resize(&p,connection,lease_epoch,cols,rows)).await.map_err(|_|ErrorCode::Internal)??;
            serde_json::json!({"type":"resized","request_id":request_id,"session_id":session_id})
        },
        ControlMessage::Input{session_id,agent_epoch,generation,lease_epoch,input_id,input_seq,payload,..}=>{
            if children.len()>=128{return Err(ErrorCode::RateLimited);}let s=state.sessions.get(session_id)?;s.validate_generation(agent_epoch,generation)?;
            let data=match payload{InputPayload::Utf8{text}=>text.into_bytes(),InputPayload::Binary{base64}=>STANDARD.decode(base64).map_err(|_|ErrorCode::InvalidRequest)?};
            let done=s.enqueue(p.clone(),connection,lease_epoch,input_id,input_seq,data,received)?;let tx=tx.clone();
            // Completion acknowledgements do not block processing the next keyboard event.
            children.spawn(async move{let value=match done.await{
                Ok(Ok(us))=>serde_json::json!({"type":"written","request_id":request_id,"session_id":session_id,"input_id":input_id,"agent_receive_to_write_us":us}),
                Ok(Err(code))=>serde_json::json!({"type":"delivery_failed","request_id":request_id,"session_id":session_id,"input_id":input_id,"code":code}),
                Err(_)=>serde_json::json!({"type":"delivery_unknown","request_id":request_id,"session_id":session_id,"input_id":input_id}),
            };let _=tx.try_send(value);});
            serde_json::json!({"type":"accepted","request_id":request_id,"session_id":session_id,"input_id":input_id})
        },
    };
    tx.try_send(reply).map_err(|_|ErrorCode::RateLimited)?;Ok(())
}
async fn send_frame(socket:&mut WebSocket,frame:Frame)->Result<(),()>{
    let bytes=frame.encode().map_err(|_|())?;
    tokio::time::timeout(Duration::from_secs(5),socket.send(Message::Binary(bytes.into()))).await.map_err(|_|())?.map_err(|_|())
}
async fn terminal(state:Shared,mut p:Principal,id:Uuid,mut socket:WebSocket){
    let Ok(session)=state.sessions.get(id)else{return;};
    let s=session.clone();let Ok(Ok((meta,snapshot,mut events)))=tokio::task::spawn_blocking(move||s.subscribe_snapshot()).await else{return;};
    let sequence=meta.sequence.parse::<u64>().unwrap_or(0);let generation=meta.generation;
    let initial=Frame{kind:frame::SNAPSHOT_META,session_id:id,generation,sequence,payload:Bytes::from(serde_json::to_vec(&meta).unwrap_or_default())};
    if send_frame(&mut socket,initial).await.is_err(){return;}
    for chunk in snapshot.chunks(frame::MAX_PAYLOAD){
        if p.require(Scope::TerminalRead).is_err(){return;}
        if send_frame(&mut socket,Frame{kind:frame::SNAPSHOT_CHUNK,session_id:id,generation,sequence,payload:Bytes::copy_from_slice(chunk)}).await.is_err(){return;}
    }
    if send_frame(&mut socket,Frame{kind:frame::SNAPSHOT_END,session_id:id,generation,sequence,payload:Bytes::new()}).await.is_err(){return;}
    // One reader has its own credit budget. A stalled peer never controls PTY consumption.
    let(mut applied,mut sent)=(sequence,sequence);let mut inflight=std::collections::VecDeque::<(u64,usize)>::new();let mut unacked=0usize;
    let mut pending=std::collections::VecDeque::<Arc<Frame>>::new();let mut deadline=tokio::time::Instant::now()+Duration::from_secs(20);
    loop{
        if p.require(Scope::TerminalRead).is_err()||state.shutdown.is_cancelled(){break;}
        if pending.is_empty(){match session.ring_after(sent){Ok(v)=>pending.extend(v),Err(_)=>break}}
        if unacked<256*1024{
            if let Some(f)=pending.pop_front(){
                if f.sequence!=sent+1{break;}let n=f.payload.len()+frame::HEADER_LEN;
                if send_frame(&mut socket,(*f).clone()).await.is_err(){break;}sent=f.sequence;unacked+=n;inflight.push_back((sent,n));continue;
            }
        }
        tokio::select!{
            biased;
            _=p.revoked.cancelled()=>break,
            _=state.shutdown.cancelled()=>break,
            _=tokio::time::sleep_until(p.expires.into())=>break,
            message=socket.recv()=>{
                let Some(Ok(Message::Text(raw)))=message else{break;};
                match serde_json::from_str::<StreamMessage>(&raw){
                    Ok(StreamMessage::RefreshAuth{ticket})=>{
                        match state.auth.consume_ws(&ticket,"terminal"){
                            Ok((next,Some(resource)))if next.client_id==p.client_id&&resource==id=>{p=next;},_=>break,
                        }
                    },
                    Ok(StreamMessage::Applied{sequence,bytes:_})=>{
                        let Ok(seq)=sequence.parse::<u64>()else{break;};if seq<applied||seq>sent{break;}applied=seq;
                        // Account from server-sent records, not a client-supplied arbitrary byte grant.
                        while inflight.front().is_some_and(|(s,_)|*s<=seq){unacked-=inflight.pop_front().unwrap().1;}
                        deadline=tokio::time::Instant::now()+Duration::from_secs(20);
                    },Ok(StreamMessage::Ping)=>{},Err(_)=>break,
                }
            },
            event=events.recv(),if unacked<256*1024=>match event{
                Ok(f)=>{if f.sequence>sent{if f.sequence==sent+1{pending.push_back(f);}else{match session.ring_after(sent){Ok(v)=>pending.extend(v),Err(_)=>break}}}},
                Err(broadcast::error::RecvError::Lagged(_))=>match session.ring_after(sent){Ok(v)=>pending.extend(v),Err(_)=>break},Err(_)=>break,
            },
            _=tokio::time::sleep_until(deadline),if unacked>0=>break,
        }
        if pending.iter().map(|f|f.payload.len()+frame::HEADER_LEN).sum::<usize>()>1024*1024{break;}
    }
    // A fresh connection uses a new authoritative snapshot; never splice missing ANSI bytes.
    let _=socket.send(Message::Close(None)).await;
}
