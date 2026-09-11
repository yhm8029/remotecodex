#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use tauri_plugin_opener::OpenerExt;
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MediaApproval{kind:String,handle:String,control:bool}
#[tauri::command]
async fn local_admin(operation:String,request:Option<MediaApproval>)->Result<serde_json::Value,String>{
    if !["status","pair_owner","pair_reader","pair_gui","media_sources","media_approve","media_clear","block_remote","resume_remote"].contains(&operation.as_str()){return Err("Unsupported local operation".into());}
    let payload=if operation=="media_approve"{
        let r=request.ok_or_else(||"Missing source approval".to_string())?;
        if !["window","monitor"].contains(&r.kind.as_str())||r.handle.len()>20||r.handle.parse::<u64>().ok().is_none_or(|n|n==0){return Err("Invalid native source".into());}
        serde_json::json!({"operation":operation,"kind":r.kind,"handle":r.handle,"control":r.control})
    }else{if request.is_some(){return Err("Unexpected local arguments".into());}serde_json::json!({"operation":operation})};
    #[cfg(windows)]{
        use tokio::{io::{AsyncReadExt,AsyncWriteExt},net::windows::named_pipe::ClientOptions};
        use std::os::windows::io::AsRawHandle;
        let operation=async move{
            let name=rc_platform_windows::pipe_name().map_err(|e|e.to_string())?;
            let mut pipe=ClientOptions::new().open(name).map_err(|e|e.to_string())?;
            rc_platform_windows::verify_pipe_peer(pipe.as_raw_handle().cast(),false).map_err(|e|e.to_string())?;
            let data=serde_json::to_vec(&payload).map_err(|e|e.to_string())?;
            pipe.write_u32(data.len()as u32).await.map_err(|e|e.to_string())?;pipe.write_all(&data).await.map_err(|e|e.to_string())?;
            let size=pipe.read_u32().await.map_err(|e|e.to_string())? as usize;if size>128*1024{return Err("Oversized IPC response".into());}
            let mut bytes=vec![0;size];pipe.read_exact(&mut bytes).await.map_err(|e|e.to_string())?;
            serde_json::from_slice(&bytes).map_err(|e|e.to_string())
        };
        tokio::time::timeout(std::time::Duration::from_secs(5),operation).await.map_err(|_|"Agent IPC timed out".to_owned())?
    }
    #[cfg(not(windows))]{Err("Company host administration requires Windows".into())}
}
#[tauri::command]
fn open_preview(app:tauri::AppHandle,url:String)->Result<(),String>{
    let u=url::Url::parse(&url).map_err(|e|e.to_string())?;
    if u.scheme()!="https"||!u.host_str().is_some_and(|h|h.ends_with(".ts.net"))||!u.port().is_some_and(|p|(8444..=8451).contains(&p))||u.path()!="/_rc/bootstrap"||u.query().is_some()||!u.username().is_empty()||u.password().is_some()||u.fragment().is_none_or(|f|f.len()>128||!f.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'||b==b'-')){return Err("Not an approved preview URL shape".into());}
    app.opener().open_url(url,None::<&str>).map_err(|e|e.to_string())
}
fn main(){
    // The desktop UI NEVER spawns, owns, or terminates rc-agent / PTYs as its child lifecycle.
    tauri::Builder::default().plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![local_admin,open_preview])
        .run(tauri::generate_context!()).expect("RemoteCodex UI could not start");
}
