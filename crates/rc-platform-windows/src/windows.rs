//! FFI is deliberately isolated here. This file needs the Windows compile/integration gate.
use anyhow::{bail,Context,Result};
use std::{ffi::c_void,mem::{size_of,zeroed},path::Path,ptr::{null,null_mut}};
use sha2::{Digest,Sha256};
use windows_sys::Win32::{Foundation::*,Security::*,Security::Authorization::*,System::{Threading::*,Pipes::*},NetworkManagement::IpHelper::*,Networking::WinSock::AF_INET};

struct Handle(HANDLE);
impl Drop for Handle { fn drop(&mut self) { unsafe { CloseHandle(self.0); } } }
fn process(pid:u32)->Result<Handle>{let h=unsafe{OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,0,pid)};if h.is_null(){return Err(std::io::Error::last_os_error().into())}Ok(Handle(h))}
pub fn process_created(pid:u32)->Result<u64>{
    let h=process(pid)?;let(mut created,mut exited,mut kernel,mut user)=unsafe{(zeroed(),zeroed(),zeroed(),zeroed())};
    if unsafe{GetProcessTimes(h.0,&mut created,&mut exited,&mut kernel,&mut user)}==0{bail!("GetProcessTimes failed");}
    Ok(((created.dwHighDateTime as u64)<<32)|created.dwLowDateTime as u64)
}
fn token_sid(pid:u32)->Result<Vec<u8>>{
    let h=process(pid)?;let mut raw=null_mut();
    if unsafe{OpenProcessToken(h.0,TOKEN_QUERY,&mut raw)}==0{bail!("OpenProcessToken failed");}let token=Handle(raw);
    let mut len=0;unsafe{GetTokenInformation(token.0,TokenUser,null_mut(),0,&mut len);}
    if len==0||len>65536{bail!("Invalid token information length");}
    // WORD-aligned allocation also satisfies TOKEN_USER pointer alignment on supported x64.
    let mut storage=vec![0usize;(len as usize+size_of::<usize>()-1)/size_of::<usize>()];
    if unsafe{GetTokenInformation(token.0,TokenUser,storage.as_mut_ptr().cast(),len,&mut len)}==0{bail!("GetTokenInformation failed");}
    let user=unsafe{&*(storage.as_ptr().cast::<TOKEN_USER>())};let length=unsafe{GetLengthSid(user.User.Sid)};
    if length==0||length>1024{bail!("Invalid SID");}
    let mut sid=vec![0u8;length as usize];if unsafe{CopySid(length,sid.as_mut_ptr().cast(),user.User.Sid)}==0{bail!("CopySid failed");}Ok(sid)
}
fn sid_string(sid:&[u8])->Result<String>{
    let mut text=null_mut();if unsafe{ConvertSidToStringSidW(sid.as_ptr().cast_mut().cast(),&mut text)}==0{bail!("SID conversion failed");}
    let mut n=0;unsafe{while *text.add(n)!=0{n+=1;}let out=String::from_utf16_lossy(std::slice::from_raw_parts(text,n));LocalFree(text.cast());Ok(out)}
}
pub fn pipe_name()->Result<String>{
    let sid=token_sid(std::process::id())?;let digest=Sha256::digest(&sid);let suffix=digest[..12].iter().map(|b|format!("{b:02x}")).collect::<String>();
    Ok(format!(r"\\.\pipe\RemoteCodex-{}",suffix))
}
/// Client PID alone is not authentication: compare SID AND Windows login session.
pub fn verify_pipe_peer(handle:HANDLE,server_side:bool)->Result<()> {
    let mut pid=0;let ok=unsafe{if server_side{GetNamedPipeClientProcessId(handle,&mut pid)}else{GetNamedPipeServerProcessId(handle,&mut pid)}};
    if ok==0{bail!("Cannot verify local pipe peer");}
    if token_sid(pid)?!=token_sid(std::process::id())?{bail!("Different Windows account");}
    let(mut peer,mut me)=(0,0);if unsafe{ProcessIdToSessionId(pid,&mut peer)}==0||unsafe{ProcessIdToSessionId(std::process::id(),&mut me)}==0||peer!=me{bail!("Different interactive Windows session");}Ok(())
}
/// A protected DACL: current user + SYSTEM, never Everyone or generic authenticated users.
pub struct LocalSecurity {descriptor:PSECURITY_DESCRIPTOR}
impl LocalSecurity {
    pub fn new()->Result<Self>{let sid=sid_string(&token_sid(std::process::id())?)?;let sddl=format!("D:P(A;;GA;;;SY)(A;;GA;;;{sid})\0").encode_utf16().collect::<Vec<_>>();let mut descriptor=null_mut();if unsafe{ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(),SDDL_REVISION_1,&mut descriptor,null_mut())}==0{bail!("Build local DACL failed");}Ok(Self{descriptor})}
    pub fn directory()->Result<Self>{
        let sid=sid_string(&token_sid(std::process::id())?)?;
        // Object/container inheritance protects future SQLite/WAL/lock children too.
        let sddl=format!("D:P(A;OICI;GA;;;SY)(A;OICI;GA;;;{sid})\0").encode_utf16().collect::<Vec<_>>();
        let mut descriptor=null_mut();
        if unsafe{ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(),SDDL_REVISION_1,&mut descriptor,null_mut())}==0{bail!("Build data-directory DACL failed");}
        Ok(Self{descriptor})
    }
    pub fn attributes(&self)->SECURITY_ATTRIBUTES{SECURITY_ATTRIBUTES{nLength:size_of::<SECURITY_ATTRIBUTES>()as u32,lpSecurityDescriptor:self.descriptor,bInheritHandle:0}}
}
impl Drop for LocalSecurity{fn drop(&mut self){unsafe{LocalFree(self.descriptor);}}}
pub fn restrict_data_dir(path:&Path)->Result<()> {
    std::fs::create_dir_all(path)?;let security=LocalSecurity::directory()?;
    let mut name=path.as_os_str().to_string_lossy().encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let ok=unsafe{windows_sys::Win32::Security::SetFileSecurityW(name.as_mut_ptr(),DACL_SECURITY_INFORMATION|PROTECTED_DACL_SECURITY_INFORMATION,security.descriptor)};
    if ok==0{return Err(std::io::Error::last_os_error()).context("Protect Agent data directory");}Ok(())
}
/// Numeric IPv4 loopback only. A wildcard listener is refused rather than quietly widened.
pub fn loopback_listener_identity(port:u16)->Result<(u32,u64)>{
    let mut size=0u32;unsafe{GetExtendedTcpTable(null_mut(),&mut size,0,AF_INET as u32,TCP_TABLE_OWNER_PID_LISTENER,0);}
    if size<4||size>16*1024*1024{bail!("Invalid TCP owner table");}
    let mut data=vec![0usize;(size as usize+size_of::<usize>()-1)/size_of::<usize>()];
    let result=unsafe{GetExtendedTcpTable(data.as_mut_ptr().cast(),&mut size,0,AF_INET as u32,TCP_TABLE_OWNER_PID_LISTENER,0)};
    if result!=0{bail!("Read TCP owner table failed: {result}");}
    let table=unsafe{&*(data.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>())};let count=table.dwNumEntries as usize;
    if count>(size as usize-4)/size_of::<MIB_TCPROW_OWNER_PID>(){bail!("Truncated TCP table");}
    let rows=unsafe{std::slice::from_raw_parts(table.table.as_ptr(),count)};
    let mut found=None;
    for row in rows{
        if u16::from_be(row.dwLocalPort as u16)!=port{continue;}
        let bytes=row.dwLocalAddr.to_ne_bytes();
        if bytes==[0,0,0,0]{bail!("Upstream binds all interfaces; bind it to 127.0.0.1 first");}
        if bytes==[127,0,0,1]{if found.is_some(){bail!("Ambiguous listener ownership");}found=Some(row.dwOwningPid);}
    }
    let pid=found.ok_or_else(||anyhow::anyhow!("No IPv4 loopback listener"))?;Ok((pid,process_created(pid)?))
}

/// GUI sources must belong to the logged-in account and interactive session.
pub fn same_user_process(pid:u32)->Result<()> {
    if token_sid(pid)?!=token_sid(std::process::id())?{bail!("Different Windows account");}
    let(mut theirs,mut ours)=(0,0);if unsafe{ProcessIdToSessionId(pid,&mut theirs)}==0||unsafe{ProcessIdToSessionId(std::process::id(),&mut ours)}==0||theirs!=ours{bail!("Different Windows session");}Ok(())
}
fn integrity(pid:u32)->Result<u32>{
    let p=process(pid)?;let mut raw=null_mut();if unsafe{OpenProcessToken(p.0,TOKEN_QUERY,&mut raw)}==0{bail!("Token unavailable");}let t=Handle(raw);
    let mut len=0;unsafe{GetTokenInformation(t.0,TokenIntegrityLevel,null_mut(),0,&mut len);}
    if len==0||len>65536{bail!("Invalid integrity data");}
    let mut data=vec![0usize;(len as usize+size_of::<usize>()-1)/size_of::<usize>()];
    if unsafe{GetTokenInformation(t.0,TokenIntegrityLevel,data.as_mut_ptr().cast(),len,&mut len)}==0{bail!("Integrity unavailable");}
    unsafe{let label=&*(data.as_ptr().cast::<TOKEN_MANDATORY_LABEL>());let count=*GetSidSubAuthorityCount(label.Label.Sid);if count==0{bail!("Invalid integrity SID");}Ok(*GetSidSubAuthority(label.Label.Sid,(count-1)as u32))}
}
pub fn gui_integrity_permitted(pid:u32)->Result<()>{same_user_process(pid)?;if integrity(pid)?>integrity(std::process::id())?{bail!("Higher-integrity target is not supported");}Ok(())}
