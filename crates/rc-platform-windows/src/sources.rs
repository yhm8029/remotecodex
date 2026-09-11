//! Enumeration is available ONLY through the same-user local administration pipe.
use anyhow::{Result,bail};
use std::{mem::{size_of,zeroed},ptr::null_mut};
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::*,System::{StationsAndDesktops::*,Threading::*},UI::WindowsAndMessaging::*};
use rc_core::{geometry::Rect,media_wire::{NativeSource,LocalSource}};

pub fn input_desktop_available()->bool{unsafe{
 let d=OpenInputDesktop(0,0,DESKTOP_READOBJECTS);if d.is_null(){return false;}
 let mut name=[0u16;64];let mut size=0;
 let ok=GetUserObjectInformationW(d,UOI_NAME,name.as_mut_ptr().cast(),(name.len()*2)as u32,&mut size)!=0;
 CloseDesktop(d);ok&&String::from_utf16_lossy(&name[..name.iter().position(|x|*x==0).unwrap_or(name.len())])=="Default"
}}
pub fn virtual_desktop()->Result<Rect>{let _dpi=super::dpi::DpiGuard::enter()?;unsafe{
 let r=Rect{left:GetSystemMetrics(SM_XVIRTUALSCREEN),top:GetSystemMetrics(SM_YVIRTUALSCREEN),width:GetSystemMetrics(SM_CXVIRTUALSCREEN).max(0)as u32,height:GetSystemMetrics(SM_CYVIRTUALSCREEN).max(0)as u32};if !r.valid(){bail!("Invalid virtual desktop");}Ok(r)
}}
fn window_rect(hwnd:HWND)->Result<Rect>{unsafe{let mut r:RECT=zeroed();let mut p=POINT{x:0,y:0};if GetClientRect(hwnd,&mut r)==0||ClientToScreen(hwnd,&mut p)==0{bail!("Window client geometry unavailable");}let rect=Rect{left:p.x,top:p.y,width:(r.right-r.left).max(0)as u32,height:(r.bottom-r.top).max(0)as u32};if !rect.valid(){bail!("Empty window");}Ok(rect)}}
pub fn enumerate_sources()->Result<Vec<LocalSource>>{
 let _dpi=super::dpi::DpiGuard::enter()?;
 if !input_desktop_available(){bail!("Interactive desktop locked/unavailable");}
 // Per-monitor-v2 on the enumeration thread prevents virtualized coordinates on mixed-DPI desktops.
 let mut out=Vec::new();unsafe{
 EnumWindows(Some(window_cb),(&mut out as *mut Vec<LocalSource>)as isize);
 EnumDisplayMonitors(null_mut(),std::ptr::null(),Some(monitor_cb),(&mut out as *mut Vec<LocalSource>)as isize);
 }Ok(out)
}
unsafe extern "system" fn window_cb(hwnd:HWND,data:isize)->BOOL{
 let out=&mut *(data as *mut Vec<LocalSource>);if out.len()>=128{return 0;}
 if IsWindowVisible(hwnd)==0||IsIconic(hwnd)!=0{return 1;}
 let mut pid=0;GetWindowThreadProcessId(hwnd,&mut pid);
 if pid==0||pid==GetCurrentProcessId()||super::same_user_process(pid).is_err(){return 1;}
 let Ok(created)=super::process_created(pid)else{return 1};let Ok(rect)=window_rect(hwnd)else{return 1};
 let mut title=[0u16;256];let n=GetWindowTextW(hwnd,title.as_mut_ptr(),title.len()as i32);if n<=0{return 1;}
 out.push(LocalSource{native:NativeSource::Window{handle:(hwnd as usize as u64).to_string(),pid,created:created.to_string()},label:String::from_utf16_lossy(&title[..n as usize]),rect});1
}
unsafe extern "system" fn monitor_cb(h:HMONITOR,_:HDC,_:*mut RECT,data:isize)->BOOL{
 let out=&mut *(data as *mut Vec<LocalSource>);if out.len()>=160{return 0;}
 let mut i:MONITORINFOEXW=zeroed();i.monitorInfo.cbSize=size_of::<MONITORINFOEXW>()as u32;
 if GetMonitorInfoW(h,(&mut i as *mut MONITORINFOEXW).cast())==0{return 1;}
 let r=i.monitorInfo.rcMonitor;let device=String::from_utf16_lossy(&i.szDevice[..i.szDevice.iter().position(|x|*x==0).unwrap_or(i.szDevice.len())]);
 out.push(LocalSource{native:NativeSource::Monitor{handle:(h as usize as u64).to_string(),device:device.clone()},label:format!("Monitor {device}"),rect:Rect{left:r.left,top:r.top,width:(r.right-r.left).max(0)as u32,height:(r.bottom-r.top).max(0)as u32}});1
}
pub fn validate_source(s:&LocalSource)->Result<Rect>{
 let _dpi=super::dpi::DpiGuard::enter()?;
 if !input_desktop_available(){bail!("Desktop locked/unavailable");}
 let h=s.native.handle().map_err(anyhow::Error::msg)?;
 match &s.native{
 NativeSource::Window{pid,created,..}=>{let hwnd=h as usize as HWND;unsafe{let mut actual=0;GetWindowThreadProcessId(hwnd,&mut actual);if IsWindow(hwnd)==0||IsIconic(hwnd)!=0||IsWindowVisible(hwnd)==0||actual!=*pid{bail!("Window closed/minimized/changed");}}super::same_user_process(*pid)?;if super::process_created(*pid)?.to_string()!=*created{bail!("PID was reused");}window_rect(hwnd)},
 NativeSource::Monitor{device,..}=>{let mut i:MONITORINFOEXW=unsafe{zeroed()};i.monitorInfo.cbSize=size_of::<MONITORINFOEXW>()as u32;if unsafe{GetMonitorInfoW(h as usize as HMONITOR,(&mut i as *mut MONITORINFOEXW).cast())}==0{bail!("Monitor missing");}let d=String::from_utf16_lossy(&i.szDevice[..i.szDevice.iter().position(|x|*x==0).unwrap_or(i.szDevice.len())]);if &d!=device{bail!("Monitor identity changed");}let r=i.monitorInfo.rcMonitor;Ok(Rect{left:r.left,top:r.top,width:(r.right-r.left).max(0)as u32,height:(r.bottom-r.top).max(0)as u32})}
 }
}
