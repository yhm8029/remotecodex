use bytes::{BufMut, Bytes, BytesMut};
use uuid::Uuid;
use crate::error::ErrorCode;
pub const HEADER_LEN: usize = 40;
pub const MAX_PAYLOAD: usize = 32 * 1024;
pub const MAX_MESSAGE: usize = 256 * 1024;
pub const OUTPUT: u8 = 1;
pub const RESIZE: u8 = 2;
pub const SNAPSHOT_META: u8 = 3;
pub const SNAPSHOT_CHUNK: u8 = 4;
pub const SNAPSHOT_END: u8 = 5;
pub const EXIT: u8 = 6;

#[derive(Debug, Clone)]
pub struct Frame {
    pub kind: u8,
    pub session_id: Uuid,
    pub generation: u32,
    pub sequence: u64,
    pub payload: Bytes,
}
impl Frame {
    pub fn encode(&self) -> Result<Bytes, ErrorCode> {
        if self.payload.len() > MAX_PAYLOAD || !(1..=6).contains(&self.kind) { return Err(ErrorCode::FrameTooLarge); }
        let mut out = BytesMut::with_capacity(HEADER_LEN + self.payload.len());
        out.extend_from_slice(b"RCTM"); out.put_u8(1); out.put_u8(self.kind); out.put_u16(0);
        out.extend_from_slice(self.session_id.as_bytes()); out.put_u32(self.generation);
        out.put_u64(self.sequence); out.put_u32(self.payload.len() as u32);
        out.extend_from_slice(&self.payload); Ok(out.freeze())
    }
    pub fn decode_all(data: &[u8]) -> Result<Vec<Self>, ErrorCode> {
        if data.len() > MAX_MESSAGE { return Err(ErrorCode::FrameTooLarge); }
        let mut pos = 0; let mut result = Vec::new();
        while pos < data.len() {
            if data.len()-pos < HEADER_LEN { return Err(ErrorCode::InvalidRequest); }
            let h = &data[pos..pos+HEADER_LEN];
            if &h[..4]!=b"RCTM" || h[4]!=1 || !(1..=6).contains(&h[5]) || h[6]!=0 || h[7]!=0 { return Err(ErrorCode::InvalidRequest); }
            let len = u32::from_be_bytes(h[36..40].try_into().unwrap()) as usize;
            if len>MAX_PAYLOAD { return Err(ErrorCode::FrameTooLarge); }
            let end = pos.checked_add(HEADER_LEN + len).ok_or(ErrorCode::FrameTooLarge)?;
            if end>data.len() { return Err(ErrorCode::InvalidRequest); }
            result.push(Self { kind:h[5], session_id:Uuid::from_slice(&h[8..24]).map_err(|_| ErrorCode::InvalidRequest)?,
                generation:u32::from_be_bytes(h[24..28].try_into().unwrap()),
                sequence:u64::from_be_bytes(h[28..36].try_into().unwrap()),
                payload:Bytes::copy_from_slice(&data[pos+HEADER_LEN..end]) });
            pos=end;
        }
        Ok(result)
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn roundtrip_and_high_sequence() {
        let f=Frame{kind:OUTPUT,session_id:Uuid::new_v4(),generation:4,sequence:(1<<54)+7,payload:Bytes::from_static("한글".as_bytes())};
        let b=f.encode().unwrap(); assert_eq!(b.len(),40+6);
        let d=Frame::decode_all(&b).unwrap(); assert_eq!(d[0].sequence,f.sequence); assert_eq!(d[0].payload,f.payload);
    }
    #[test] fn rejects_partial_reserved_and_oversize() {
        let f=Frame{kind:OUTPUT,session_id:Uuid::nil(),generation:1,sequence:0,payload:Bytes::from_static(b"x")};
        let mut b=f.encode().unwrap().to_vec(); assert!(Frame::decode_all(&b[..39]).is_err());
        b[7]=1; assert!(Frame::decode_all(&b).is_err()); b[7]=0; b[36..40].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(Frame::decode_all(&b).is_err());
    }
}
