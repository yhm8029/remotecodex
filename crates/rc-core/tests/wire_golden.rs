use rc_core::frame::Frame;
#[test]
fn shared_wire_vectors_match_other_language_clients(){
    let vectors:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/wire.json")).unwrap();
    for v in vectors.as_array().unwrap(){
        let hex=v["hex"].as_str().unwrap();let bytes=(0..hex.len()).step_by(2).map(|i|u8::from_str_radix(&hex[i..i+2],16).unwrap()).collect::<Vec<_>>();
        let frames=Frame::decode_all(&bytes).unwrap();let f=&frames[0];assert_eq!(f.session_id.to_string(),v["session_id"].as_str().unwrap());assert_eq!(f.sequence.to_string(),v["sequence"].as_str().unwrap());assert_eq!(std::str::from_utf8(&f.payload).unwrap(),v["text"].as_str().unwrap());assert_eq!(&f.encode().unwrap()[..],&bytes[..]);
    }
}
