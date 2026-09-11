//! Real WGC -> D3D11 -> Media Foundation hardware H.264 -> DTLS-SRTP WebRTC pipeline.
//! Needs Windows compilation and device validation; source availability is NOT a performance pass.
use std::{io::{self,BufReader,Write},sync::{Arc,atomic::{AtomicBool,AtomicU64,Ordering}},time::{Duration,Instant}};
use anyhow::{Result,bail,Context};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_webrtc as webrtc;
use gstreamer_sdp as sdp;
use rc_core::media_wire::*;
use crossbeam_channel::{bounded,Sender};

const FACTORIES:&[&str]=&["d3d11screencapturesrc","d3d11convert","mfh264enc","h264parse","rtph264pay","webrtcbin","nicesrc","nicesink"];
pub fn probe()->super::Capabilities{
 let init=gst::init();let missing=if let Err(e)=init{vec![e.to_string()]}else{FACTORIES.iter().filter(|n|gst::ElementFactory::find(n).is_none()).map(|n|n.to_string()).collect()};
 super::Capabilities{protocol:1,native_backend:true,available:missing.is_empty(),sdk_version:Some(gst::version_string().to_string()),missing,windows_runtime_verified:false}
}
fn event(tx:&Sender<HelperOut>,stop:&AtomicBool,e:HelperOut){if tx.try_send(e).is_err(){stop.store(true,Ordering::Release);}}
fn write_event(w:&mut impl Write,e:&HelperOut)->Result<()>{let bytes=serde_json::to_vec(e)?;if bytes.len()>MAX_SIGNAL-1{bail!("Oversized signaling output");}w.write_all(&bytes)?;w.write_all(b"\n")?;w.flush()?;Ok(())}
struct Playing(gst::Pipeline);
impl Drop for Playing{fn drop(&mut self){let _=self.0.set_state(gst::State::Null);}}

pub fn run()->Result<()>{
 gst::init()?;
 let mut input=BufReader::new(io::stdin());
 let init=match read_json_line::<_,HelperIn>(&mut input)?{Some(HelperIn::Init{config})=>config,_=>bail!("First message must be Init")};init.validate().map_err(anyhow::Error::msg)?;
 if rc_platform_windows::validate_source(&init.source)?!=init.source.rect{bail!("Approved geometry changed before capture");}
 let caps=probe();if !caps.available{bail!("Missing native factories: {:?}",caps.missing);}
 let p=init.profile.fit(init.source.rect).map_err(anyhow::Error::msg)?;
 let handle=init.source.native.handle().map_err(anyhow::Error::msg)?;
 let source_arg=match init.source.native{NativeSource::Window{..}=>format!("window-handle={handle} window-capture-mode=client"),NativeSource::Monitor{..}=>format!("monitor-handle={handle}")};
 // Every substituted value is a bounded number selected locally, never a user pipeline string.
 let description=format!(
  "webrtcbin name=rtc bundle-policy=max-bundle latency=0 \
   d3d11screencapturesrc name=capture capture-api=wgc {source_arg} show-border=true show-cursor=true \
   ! video/x-raw(memory:D3D11Memory),framerate={fps}/1 \
   ! queue max-size-buffers=2 max-size-bytes=0 max-size-time=0 leaky=downstream \
   ! d3d11convert ! video/x-raw(memory:D3D11Memory),format=NV12,width={width},height={height} \
   ! mfh264enc name=encoder low-latency=true bframes=0 cabac=false bitrate={bitrate} gop-size={gop} \
   ! video/x-h264,profile=constrained-baseline \
   ! h264parse name=encoded config-interval=-1 \
   ! rtph264pay pt=96 config-interval=-1 aggregate-mode=zero-latency \
   ! application/x-rtp,media=video,encoding-name=H264,clock-rate=90000,payload=96 ! rtc.",
   fps=p.fps,width=p.width,height=p.height,bitrate=p.bitrate_kbps,gop=p.fps*2);
 let pipeline=gst::parse::launch(&description)?.downcast::<gst::Pipeline>().map_err(|_|anyhow::anyhow!("Media pipeline type mismatch"))?;
 let _guard=Playing(pipeline.clone());let rtc=pipeline.by_name("rtc").context("WebRTC element missing")?;
 let encoder=pipeline.by_name("encoder").context("Encoder missing")?;
 if !encoder.property::<bool>("d3d11-aware"){bail!("Hardware encoder cannot accept D3D11 surfaces; CPU copy fallback is not enabled");}
 let ice=rtc.property::<webrtc::WebRTCICE>("ice-agent");
 if ice.find_property("min-rtp-port").is_none()||ice.find_property("max-rtp-port").is_none(){bail!("ICE port bounds unavailable");}
 ice.set_property("min-rtp-port",init.min_port as u32);ice.set_property("max-rtp-port",init.max_port as u32);
 // This official ICE action disables automatic local-interface discovery.
 if !ice.emit_by_name::<bool>("add-local-ip-address",&[&init.tailnet_ip]){bail!("Could not bind WebRTC to the approved Tailscale IP");}
 let stop=Arc::new(AtomicBool::new(false));let frames=Arc::new(AtomicU64::new(0));
 let(tx,events)=bounded::<HelperOut>(64);let(commands,input_rx)=bounded::<HelperIn>(32);
 let s=stop.clone();std::thread::Builder::new().name("rc-media-input".into()).spawn(move||{
  loop{match read_json_line::<_,HelperIn>(&mut input){Ok(Some(m))=>{let end=matches!(m,HelperIn::Stop);if commands.send(m).is_err()||end{break;}},_=>break}}s.store(true,Ordering::Release);
 })?;
 let pending=Arc::new(AtomicBool::new(false));let first=pending.clone();let t=tx.clone();let s=stop.clone();
 rtc.connect("on-negotiation-needed",false,move|values|{
  if first.swap(true,Ordering::AcqRel){return None;}
  let Ok(rtc)=values[0].get::<gst::Element>()else{s.store(true,Ordering::Release);return None};
  let w=rtc.downgrade();let t=t.clone();let s=s.clone();
  let promise=gst::Promise::with_change_func(move|reply|{
   let result=(||->Result<()>{let r=reply.map_err(|e|anyhow::anyhow!("Offer promise: {e:?}"))?.context("No SDP offer")?;let offer=r.get::<webrtc::WebRTCSessionDescription>("offer")?;
    let rtc=w.upgrade().context("WebRTC element gone")?;rtc.emit_by_name::<()>("set-local-description",&[&offer,&None::<gst::Promise>]);
    let text=sanitize_sdp(&offer.sdp().as_text()?).map_err(anyhow::Error::msg)?;event(&t,&s,HelperOut::Offer{sdp:text});Ok(())})();
   if result.is_err(){event(&t,&s,HelperOut::Error{code:"OFFER_FAILED".into()});s.store(true,Ordering::Release);}
  });rtc.emit_by_name::<()>("create-offer",&[&None::<gst::Structure>,&promise]);None
 });
 let ip=init.tailnet_ip.clone();let t=tx.clone();let s=stop.clone();
 rtc.connect("on-ice-candidate",false,move|values|{
  if let(Ok(mline),Ok(candidate))=(values[1].get::<u32>(),values[2].get::<String>()){
   if mline==0&&candidate_allowed(&candidate,Some(&ip)){event(&t,&s,HelperOut::Ice{candidate,mline});}
  }None
 });
 let counter=frames.clone();pipeline.by_name("encoded").context("Parser missing")?.static_pad("src").context("Encoder pad missing")?.add_probe(gst::PadProbeType::BUFFER,move|_,_|{counter.fetch_add(1,Ordering::Relaxed);gst::PadProbeReturn::Ok});
 let bus=pipeline.bus().context("Pipeline has no bus")?;let t=tx.clone();let s=stop.clone();
 bus.set_sync_handler(move|_,m|{match m.view(){gst::MessageView::Error(_)=>{event(&t,&s,HelperOut::Error{code:"NATIVE_PIPELINE_FAILED".into()});s.store(true,Ordering::Release);},gst::MessageView::Eos(_)=>{s.store(true,Ordering::Release);},_=>{}}gst::BusSyncReply::Drop});
 let mut output=io::stdout().lock();write_event(&mut output,&HelperOut::Ready{protocol:1})?;
 pipeline.set_state(gst::State::Playing)?;
 let mut last_sent=0;let mut last_publish=Instant::now();let mut answer_seen=false;
 while !stop.load(Ordering::Acquire){
  crossbeam_channel::select!{
   recv(events)->m=>{if let Ok(m)=m{write_event(&mut output,&m)?;}else{break;}},
   recv(input_rx)->m=>{match m{
    Ok(HelperIn::Answer{sdp:answer})=>{if answer_seen{bail!("Renegotiation is not supported in this stream");}let clean=sanitize_sdp(&answer).map_err(anyhow::Error::msg)?;let parsed=sdp::SDPMessage::parse_buffer(clean.as_bytes())?;let desc=webrtc::WebRTCSessionDescription::new(webrtc::WebRTCSDPType::Answer,parsed);rtc.emit_by_name::<()>("set-remote-description",&[&desc,&None::<gst::Promise>]);answer_seen=true;},
    Ok(HelperIn::Ice{candidate,mline})=>{if mline!=0||!candidate_allowed(&candidate,None){bail!("ICE candidate rejected");}rtc.emit_by_name::<()>("add-ice-candidate",&[&mline,&candidate]);},
    Ok(HelperIn::Stop)|Err(_)=>break,
    Ok(HelperIn::Init{..})=>bail!("Duplicate init"),
   }},
   default(Duration::from_millis(50))=>{}
  }
  if last_publish.elapsed()>=Duration::from_millis(100){let n=frames.load(Ordering::Relaxed);if n!=last_sent{write_event(&mut output,&HelperOut::Frame{sequence:n.to_string()})?;last_sent=n;}last_publish=Instant::now();}
 }
 // Only metadata has used stdout; all video was sent by WebRTC. Drop sets the pipeline to NULL.
 pipeline.set_state(gst::State::Null)?;write_event(&mut output,&HelperOut::Stopped)?;Ok(())
}
