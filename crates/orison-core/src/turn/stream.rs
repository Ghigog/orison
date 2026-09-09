//! Reading display text out of a response that is still arriving.
//!
//! The Actor's response is one schema-constrained JSON object, so a naive
//! stream would show the player `{"thinking":"The guard sizes them up`. The
//! Godot build solved this with `LLMStreamParser.gd`, which watches the token
//! stream for `"narration":` or `"dialogue":` and forwards the characters
//! inside the string that follows.
//!
//! This is that idea with the two things it gets wrong fixed:
//!
//! - **Escapes are decoded.** The Godot parser forwards the raw bytes, so a
//!   response containing `\n` or `\"` shows the player a literal backslash-n.
//!   Only its unescaped-quote check looks at escapes at all, and only to find
//!   the end of the string.
//! - **Chunk boundaries are not assumed.** A field name, an escape, or a
//!   `\uXXXX` sequence split across two tokens is ordinary; the Godot parser
//!   handles the first case by accident (its buffer keeps growing) and the
//!   other two not at all.
//!
//! What reaches the player during streaming is therefore byte-identical to
//! what the parsed [`CharacterResponse`] field holds when the response
//! completes.
//!
//! [`CharacterResponse`]: crate::prompt::schemas::CharacterResponse

/// One thing that happened while a chunk was being read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamPiece {
    /// A watched field's value started.
    ZoneStarted(&'static str),
    /// Decoded text from inside the field currently open.
    Text(String),
    /// The field's value ended.
    ZoneEnded(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Escape {
    /// Just saw a backslash.
    Pending,
    /// Inside `\uXXXX`, with this many hex digits collected.
    Unicode(u8),
}

/// Extracts the values of named string fields from a JSON document that is
/// still arriving.
#[derive(Debug)]
pub struct FieldStreamer {
    fields: Vec<&'static str>,
    /// Bytes seen since the last field value closed, still being searched.
    buffer: String,
    open: Option<&'static str>,
    escape: Option<Escape>,
    unicode: String,
}

impl FieldStreamer {
    /// Watch `fields`, in the document's own order of preference: the first
    /// one whose value starts is the one opened.
    pub fn new(fields: &[&'static str]) -> Self {
        Self {
            fields: fields.to_vec(),
            buffer: String::new(),
            open: None,
            escape: None,
            unicode: String::new(),
        }
    }

    /// Forget everything. Called between turns.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.open = None;
        self.escape = None;
        self.unicode.clear();
    }

    /// Feed one delta and get back what it produced.
    pub fn push(&mut self, chunk: &str) -> Vec<StreamPiece> {
        let mut out = Vec::new();
        self.ingest(chunk, &mut out);
        out
    }

    fn ingest(&mut self, chunk: &str, out: &mut Vec<StreamPiece>) {
        if self.open.is_some() {
            self.consume_string(chunk, out);
            return;
        }

        self.buffer.push_str(chunk);
        let Some((field, value_start)) = self.find_open_field() else {
            return;
        };
        // `split_off` leaves the buffer holding everything before the value
        // and hands us the rest; the buffer is cleared because a field's
        // value can never contain the start of another field.
        let remainder = self.buffer.split_off(value_start);
        self.buffer.clear();
        self.open = Some(field);
        out.push(StreamPiece::ZoneStarted(field));
        self.consume_string(&remainder, out);
    }

    /// The earliest watched field whose `"name":` is followed by an opening
    /// quote, and the byte offset just past that quote.
    ///
    /// Returns `None` while the quote has not arrived yet, which is the
    /// straddled-boundary case: the buffer is kept and the next chunk retries.
    fn find_open_field(&self) -> Option<(&'static str, usize)> {
        let mut best: Option<(&'static str, usize)> = None;
        for field in &self.fields {
            let tag = format!("\"{field}\":");
            let Some(at) = self.buffer.find(&tag) else {
                continue;
            };
            let after = at + tag.len();
            let rest = &self.buffer[after..];
            let trimmed = rest.trim_start();
            if !trimmed.starts_with('"') {
                continue;
            }
            let quote_at = after + (rest.len() - trimmed.len()) + 1;
            if best.is_none_or(|(_, existing)| quote_at < existing) {
                best = Some((field, quote_at));
            }
        }
        best
    }

    /// Read text out of the open string until it closes, then go back to
    /// searching with whatever is left.
    fn consume_string(&mut self, text: &str, out: &mut Vec<StreamPiece>) {
        let mut decoded = String::with_capacity(text.len());
        for (idx, c) in text.char_indices() {
            match self.escape {
                Some(Escape::Pending) => {
                    self.escape = None;
                    match c {
                        'n' => decoded.push('\n'),
                        't' => decoded.push('\t'),
                        'r' => decoded.push('\r'),
                        'b' => decoded.push('\u{8}'),
                        'f' => decoded.push('\u{c}'),
                        'u' => {
                            self.escape = Some(Escape::Unicode(0));
                            self.unicode.clear();
                        }
                        // `"`, `\`, `/` and anything a constrained decoder
                        // should never emit: pass the character through
                        // rather than dropping it.
                        other => decoded.push(other),
                    }
                }
                Some(Escape::Unicode(seen)) => {
                    self.unicode.push(c);
                    let seen = seen + 1;
                    if seen < 4 {
                        self.escape = Some(Escape::Unicode(seen));
                    } else {
                        self.escape = None;
                        if let Some(ch) = u32::from_str_radix(&self.unicode, 16)
                            .ok()
                            .and_then(char::from_u32)
                        {
                            decoded.push(ch);
                        }
                        self.unicode.clear();
                    }
                }
                None => match c {
                    '\\' => self.escape = Some(Escape::Pending),
                    '"' => {
                        // The field closed. Everything after it is document
                        // again, and may open the next watched field in the
                        // very same chunk.
                        let field = self
                            .open
                            .take()
                            .expect("in a string implies a field is open");
                        if !decoded.is_empty() {
                            out.push(StreamPiece::Text(std::mem::take(&mut decoded)));
                        }
                        out.push(StreamPiece::ZoneEnded(field));
                        let rest_at = idx + c.len_utf8();
                        let rest = text[rest_at..].to_string();
                        if !rest.is_empty() {
                            self.ingest(&rest, out);
                        }
                        return;
                    }
                    other => decoded.push(other),
                },
            }
        }

        if !decoded.is_empty() {
            out.push(StreamPiece::Text(decoded));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(pieces: &[StreamPiece]) -> String {
        pieces
            .iter()
            .filter_map(|p| match p {
                StreamPiece::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect()
    }

    /// The whole point: what the player watched arrive equals what the
    /// parsed field holds.
    #[test]
    fn streamed_text_matches_the_parsed_field() {
        let document = serde_json::json!({
            "thinking": "They asked about the accord.",
            "narration": "The guard shifts.\nSteel taps stone.",
            "dialogue": "\"The accord?\" I say. Nobody speaks of it.",
        });
        let serialized = serde_json::to_string(&document).unwrap();

        let mut streamer = FieldStreamer::new(&["narration", "dialogue"]);
        let mut narration = String::new();
        let mut dialogue = String::new();
        let mut zone: Option<&str> = None;

        // One character at a time is the worst case a real token stream can
        // present, so it is the case worth testing.
        for c in serialized.chars() {
            for piece in streamer.push(&c.to_string()) {
                match piece {
                    StreamPiece::ZoneStarted(f) => zone = Some(f),
                    StreamPiece::ZoneEnded(_) => zone = None,
                    StreamPiece::Text(t) => match zone {
                        Some("narration") => narration.push_str(&t),
                        Some("dialogue") => dialogue.push_str(&t),
                        _ => panic!("text outside a zone: {t:?}"),
                    },
                }
            }
        }

        assert_eq!(narration, document["narration"].as_str().unwrap());
        assert_eq!(dialogue, document["dialogue"].as_str().unwrap());
    }

    #[test]
    fn escapes_are_decoded_rather_than_forwarded_raw() {
        let mut streamer = FieldStreamer::new(&["dialogue"]);
        let pieces = streamer.push(r#"{"dialogue":"a\nb\"céd""#);
        assert_eq!(text_of(&pieces), "a\nb\"céd");
    }

    #[test]
    fn a_unicode_escape_split_across_chunks_still_decodes() {
        let mut streamer = FieldStreamer::new(&["dialogue"]);
        let mut all = Vec::new();
        for chunk in [r#"{"dialogue":"caf\u"#, "00", r#"e9 ferme""#] {
            all.extend(streamer.push(chunk));
        }
        assert_eq!(text_of(&all), "café ferme");
    }

    #[test]
    fn a_field_name_split_across_chunks_is_still_found() {
        let mut streamer = FieldStreamer::new(&["dialogue"]);
        let mut all = Vec::new();
        for chunk in [r#"{"thinking":"x","dial"#, r#"ogue": "#, r#""hello""#] {
            all.extend(streamer.push(chunk));
        }
        assert_eq!(all.first(), Some(&StreamPiece::ZoneStarted("dialogue")));
        assert_eq!(text_of(&all), "hello");
    }

    #[test]
    fn thinking_is_never_shown_to_the_player() {
        // `thinking` is a mandatory field of the response and pure reasoning.
        // It is not in the watched set, so it must produce nothing at all.
        let mut streamer = FieldStreamer::new(&["narration", "dialogue"]);
        let pieces = streamer.push(r#"{"thinking":"They are lying about the ledger.","#);
        assert!(pieces.is_empty(), "{pieces:?}");
    }

    #[test]
    fn both_zones_can_open_within_one_chunk() {
        let mut streamer = FieldStreamer::new(&["narration", "dialogue"]);
        let pieces = streamer.push(r#"{"narration":"He stands.","dialogue":"Sit.""#);
        assert_eq!(
            pieces,
            vec![
                StreamPiece::ZoneStarted("narration"),
                StreamPiece::Text("He stands.".into()),
                StreamPiece::ZoneEnded("narration"),
                StreamPiece::ZoneStarted("dialogue"),
                StreamPiece::Text("Sit.".into()),
                StreamPiece::ZoneEnded("dialogue"),
            ]
        );
    }
}
