//! Actual Windows input backend; only called by the visible, guarded GUI session thread.
//! Window selection is NOT an OS sandbox. Focus and UIPI races remain explicit limitations.
use std::{collections::HashSet,mem::{size_of,zeroed},time::{Duration,Instant}};
use anyhow::{Result,bail};
use windows_sys::Win32::{Foundation::*,UI::{WindowsAndMessaging::*,Input::KeyboardAndMouse::*}};
use rc_core::{geometry::Rect,media_wire::{LocalSource,NativeSource,GuiAction}};
/// Own injections are ignored by the safety hook; no physical-input content is recorded.
pub const INPUT_TAG:usize=0x5243_4755;
pub struct InputBackend {source:LocalSource,desktop:Rect,last_frame:Instant,keys:HashSet<(u16,bool)>,buttons:HashSet<u8>,enabled:bool}
impl InputBackend{
 pub fn new(source:LocalSource)->Result<Self>{Ok(Self{source,desktop:super::virtual_desktop()?,last_frame:Instant::now()-Duration::from_secs(5),keys:HashSet::new(),buttons:HashSet::new(),enabled:false})}
 pub fn frame(&mut self){self.last_frame=Instant::now();}
 pub fn arm(&mut self)->Result<()>{self.check()?;self.enabled=true;Ok(())}
 pub fn disarm(&mut self){self.enabled=false;self.release_injected();}
 fn check(&self)->Result<()>{
  if self.last_frame.elapsed()>Duration::from_millis(500)||!super::input_desktop_available(){bail!("Frame stale or desktop locked");}
  if super::validate_source(&self.source)?!=self.source.rect||super::virtual_desktop()?!=self.desktop{bail!("Geometry changed");}
  let fg=unsafe{GetForegroundWindow()};if fg.is_null(){bail!("No foreground target");}
  if let NativeSource::Window{handle,..}=&self.source.native{if fg as usize as u64!=handle.parse::<u64>()?{bail!("Selected window is not foreground; focus is never stolen");}}
  let mut pid=0;unsafe{GetWindowThreadProcessId(fg,&mut pid);}super::gui_integrity_permitted(pid)?;Ok(())
 }
 pub fn check_live(&mut self)->Result<()>{if let Err(e)=self.check(){self.disarm();return Err(e);}Ok(())}
 fn send(inputs:&[INPUT])->Result<()>{if unsafe{SendInput(inputs.len()as u32,inputs.as_ptr(),size_of::<INPUT>()as i32)}!=inputs.len()as u32{bail!("SendInput partial/failed; never replay");}Ok(())}
 fn mouse(flags:u32,dx:i32,dy:i32,data:u32)->INPUT{let mut i:INPUT=unsafe{zeroed()};i.r#type=INPUT_MOUSE;i.Anonymous.mi=MOUSEINPUT{dx,dy,mouseData:data,dwFlags:flags,time:0,dwExtraInfo:INPUT_TAG};i}
 fn key(scan:u16,extended:bool,down:bool,unicode:bool)->INPUT{let mut i:INPUT=unsafe{zeroed()};i.r#type=INPUT_KEYBOARD;i.Anonymous.ki=KEYBDINPUT{wVk:0,wScan:scan,dwFlags:if unicode{KEYEVENTF_UNICODE}else{KEYEVENTF_SCANCODE}|if extended{KEYEVENTF_EXTENDEDKEY}else{0}|if down{0}else{KEYEVENTF_KEYUP},time:0,dwExtraInfo:INPUT_TAG};i}
 fn move_to(&self,x:f64,y:f64)->Result<()>{
  let(x,y)=self.source.rect.map(x,y).map_err(|e|anyhow::anyhow!("{e:?}"))?;
  // Verify the actual point's root window before a click can follow it.
  let h=unsafe{GetAncestor(WindowFromPoint(POINT{x,y}),GA_ROOT)};
  if h.is_null(){bail!("Point has no target");}let mut pid=0;unsafe{GetWindowThreadProcessId(h,&mut pid);}super::gui_integrity_permitted(pid)?;
  if let NativeSource::Window{handle,..}=&self.source.native{if h as usize as u64!=handle.parse::<u64>()?{bail!("Selected window is covered by another window");}}
  let(ax,ay)=self.desktop.absolute(x,y).map_err(|e|anyhow::anyhow!("{e:?}"))?;Self::send(&[Self::mouse(MOUSEEVENTF_MOVE|MOUSEEVENTF_ABSOLUTE|MOUSEEVENTF_VIRTUALDESK,ax,ay,0)])
 }
 pub fn apply(&mut self,a:GuiAction,allowed:impl Fn()->bool)->Result<()>{
  if !self.enabled||!allowed(){bail!("GUI not armed or locally revoked");}a.validate().map_err(anyhow::Error::msg)?;self.check_live()?;
  let result=(||{match a{
   GuiAction::Move{x,y}=>self.move_to(x,y),
   GuiAction::Button{x,y,button,down}=>{self.move_to(x,y)?;if !allowed(){bail!("Input revoked before click");}Self::send(&[Self::mouse(button_flag(button,down)?,0,0,0)])?;if down{self.buttons.insert(button);}else{self.buttons.remove(&button);}Ok(())},
   GuiAction::Wheel{x,y,delta}=>{self.move_to(x,y)?;if !allowed(){bail!("Input revoked before wheel");}Self::send(&[Self::mouse(MOUSEEVENTF_WHEEL,0,0,delta as u32)])},
   GuiAction::Key{scan,extended,down}=>{
    if down&&scan==83&&extended&&self.keys.iter().any(|(s,_)|*s==29)&&self.keys.iter().any(|(s,_)|*s==56){bail!("Secure attention sequence not supported");}
    if !allowed(){bail!("Input revoked before key");}Self::send(&[Self::key(scan,extended,down,false)])?;if down{self.keys.insert((scan,extended));}else{self.keys.remove(&(scan,extended));}Ok(())
   },
   GuiAction::Text{text}=>{
    for c in text.encode_utf16(){if !allowed(){bail!("Text interrupted by local input");}self.check_live()?;let down=Self::key(c,false,true,true);let up=Self::key(c,false,false,true);if let Err(e)=Self::send(&[down,up]){let _=Self::send(&[up]);return Err(e);}}Ok(())
   }
  }})();if result.is_err(){self.disarm();}result
 }
 fn release_injected(&mut self){for(scan,ext)in self.keys.drain(){let _=Self::send(&[Self::key(scan,ext,false,false)]);}for button in self.buttons.drain(){if let Ok(flag)=button_flag(button,false){let _=Self::send(&[Self::mouse(flag,0,0,0)]);}}}
}
fn button_flag(b:u8,down:bool)->Result<u32>{match(b,down){(0,true)=>Ok(MOUSEEVENTF_LEFTDOWN),(0,false)=>Ok(MOUSEEVENTF_LEFTUP),(1,true)=>Ok(MOUSEEVENTF_MIDDLEDOWN),(1,false)=>Ok(MOUSEEVENTF_MIDDLEUP),(2,true)=>Ok(MOUSEEVENTF_RIGHTDOWN),(2,false)=>Ok(MOUSEEVENTF_RIGHTUP),_=>bail!("Unsupported mouse button")}}
impl Drop for InputBackend{fn drop(&mut self){self.disarm();}}
