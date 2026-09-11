/// A bounded framing/security fence, NOT a terminal emulator.
/// It only releases complete UTF-8 scalars and complete ESC sequences. This
/// allows snapshots between emitted chunks without losing partial parser state.
/// OSC 52, OSC 8 and DCS payloads are omitted from this initial safe profile.
/// alacritty_terminal remains the only VT state engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Ground,
    Esc,
    Csi,
    Osc,
    OscEsc,
    String,
    StringEsc,
}
#[derive(Debug)]
pub struct VtFence {
    state: State,
    pending: Vec<u8>,
    utf8: Vec<u8>,
    discard: bool,
    pub omitted: u64,
}
impl Default for VtFence {
    fn default() -> Self {
        Self {
            state: State::Ground,
            pending: Vec::new(),
            utf8: Vec::new(),
            discard: false,
            omitted: 0,
        }
    }
}
impl VtFence {
    fn push(&mut self, b: u8) {
        if self.pending.len() < 16 * 1024 {
            self.pending.push(b);
        } else {
            self.discard = true;
        }
    }
    fn complete(&mut self, out: &mut Vec<Vec<u8>>) {
        let unsafe_osc =
            self.pending.starts_with(b"\x1b]52;") || self.pending.starts_with(b"\x1b]8;");
        if self.discard || unsafe_osc {
            self.omitted += 1;
        } else if !self.pending.is_empty() {
            out.push(std::mem::take(&mut self.pending));
        }
        self.pending.clear();
        self.state = State::Ground;
        self.discard = false;
    }
    pub fn push_bytes(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let mut text = Vec::new();
        for &b in bytes {
            match self.state {
                State::Ground => {
                    if !self.utf8.is_empty() {
                        self.utf8.push(b);
                        match std::str::from_utf8(&self.utf8) {
                            Ok(_) => {
                                text.extend_from_slice(&self.utf8);
                                self.utf8.clear();
                            }
                            Err(e) if e.error_len().is_some() => {
                                text.extend_from_slice("�".as_bytes());
                                self.utf8.clear();
                                if b < 0x80 {
                                    if b == 0x1b {
                                        if !text.is_empty() {
                                            out.push(std::mem::take(&mut text));
                                        }
                                        self.pending.push(b);
                                        self.state = State::Esc;
                                    } else {
                                        text.push(b);
                                    }
                                }
                            }
                            _ => {}
                        }
                    } else if b == 0x1b {
                        if !text.is_empty() {
                            out.push(std::mem::take(&mut text));
                        }
                        self.pending.push(b);
                        self.state = State::Esc;
                    } else if b >= 0xc2 {
                        self.utf8.push(b);
                    } else if b >= 0x80 {
                        text.extend_from_slice("�".as_bytes());
                    } else {
                        text.push(b);
                    }
                }
                State::Esc => {
                    self.push(b);
                    match b {
                        b'[' => self.state = State::Csi,
                        b']' => self.state = State::Osc,
                        b'P' | b'_' | b'^' | b'X' => {
                            self.state = State::String;
                            self.discard = true;
                        }
                        0x20..=0x2f => {}
                        _ => self.complete(&mut out),
                    }
                }
                State::Csi => {
                    self.push(b);
                    if (0x40..=0x7e).contains(&b) {
                        self.complete(&mut out);
                    } else if b == 0x18 || b == 0x1a {
                        self.discard = true;
                        self.complete(&mut out);
                    } else if self.pending.len() > 1024 {
                        self.discard = true;
                    }
                }
                State::Osc => {
                    self.push(b);
                    if b == 7 {
                        self.complete(&mut out);
                    } else if b == 0x1b {
                        self.state = State::OscEsc;
                    }
                }
                State::OscEsc => {
                    self.push(b);
                    if b == b'\\' {
                        self.complete(&mut out);
                    } else {
                        self.state = State::Osc;
                    }
                }
                State::String => {
                    self.push(b);
                    if b == 0x1b {
                        self.state = State::StringEsc;
                    }
                }
                State::StringEsc => {
                    self.push(b);
                    if b == b'\\' {
                        self.complete(&mut out);
                    } else {
                        self.state = State::String;
                    }
                }
            }
        }
        if !text.is_empty() {
            out.push(text);
        }
        out
    }
    pub fn pending_len(&self) -> usize {
        self.pending.len() + self.utf8.len()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunks_keep_korean_intact() {
        let mut f = VtFence::default();
        let mut out = Vec::new();
        for b in "한글🙂".as_bytes() {
            for t in f.push_bytes(&[*b]) {
                out.extend(t);
            }
        }
        assert_eq!(out, "한글🙂".as_bytes());
    }
    #[test]
    fn partial_escape_waits() {
        let mut f = VtFence::default();
        assert!(f.push_bytes(b"\x1b[31").is_empty());
        assert_eq!(f.push_bytes(b"mX").concat(), b"\x1b[31mX");
    }
    #[test]
    fn malicious_osc_is_not_sent() {
        let mut f = VtFence::default();
        assert_eq!(f.push_bytes(b"A\x1b]52;c;evil\x07B").concat(), b"AB");
        assert_eq!(f.omitted, 1);
    }
    #[test]
    fn string_limit() {
        let mut f = VtFence::default();
        f.push_bytes(b"\x1bP");
        f.push_bytes(&vec![b'x'; 100000]);
        assert!(f.pending_len() <= 16384);
        assert_eq!(f.push_bytes(b"\x1b\\OK").concat(), b"OK");
    }
}
