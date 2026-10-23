pub mod buffer;
pub mod config;
pub mod ipc;
pub mod spellcheck;
pub mod syllable;
pub mod tone;

pub use buffer::{Diff, EngineConfig, WordBuffer};
pub use config::{AppRule, CommitMode, Config, ToggleShortcut};
pub use ipc::{DaemonStatus, IpcRequest, IpcResponse, NiriWindow, runtime_socket_path};
pub use syllable::{BaseVowel, InputMethod, Tone, ToneStyle, VowelChar, VowelMod};

/// Helper function to transform a full space-separated text string using `EngineConfig`.
pub fn transform_text(input: &str, cfg: &EngineConfig) -> String {
    let mut out = String::with_capacity(input.len());
    let mut buf = WordBuffer::new();

    for c in input.chars() {
        if c.is_alphabetic() || (cfg.input_method == InputMethod::Vni && c.is_ascii_digit()) {
            let diff = buf.push_char(c, cfg);
            apply_diff(&mut out, &diff);
        } else {
            let fin_diff = buf.finalize_word(cfg);
            apply_diff(&mut out, &fin_diff);
            out.push(c);
        }
    }
    let fin_diff = buf.finalize_word(cfg);
    apply_diff(&mut out, &fin_diff);
    out
}

fn apply_diff(target: &mut String, diff: &Diff) {
    for _ in 0..diff.backspace_chars {
        target.pop();
    }
    target.push_str(&diff.insert_text);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn telex_modern() -> EngineConfig {
        EngineConfig {
            input_method: InputMethod::Telex,
            tone_style: ToneStyle::Modern,
            spellcheck: true,
            auto_restore_english: true,
            w_at_word_start_as_u_horn: true,
        }
    }

    fn telex_traditional() -> EngineConfig {
        EngineConfig {
            tone_style: ToneStyle::Traditional,
            ..telex_modern()
        }
    }

    fn vni_modern() -> EngineConfig {
        EngineConfig {
            input_method: InputMethod::Vni,
            ..telex_modern()
        }
    }

    #[test]
    fn test_basic_telex_sentences() {
        let cfg = telex_modern();
        assert_eq!(
            transform_text("Vieetj Nam ddaats nuowcs con nguowif", &cfg),
            "Việt Nam đất nước con người"
        );
        assert_eq!(
            transform_text("Xin chaof cacs banj minhf laf ViKey", &cfg),
            "Xin chào các bạn mình là ViKey"
        );
        assert_eq!(
            transform_text("chuyeenj gif cungx coos gawngs", &cfg),
            "chuyện gì cũng cố gắng"
        );
        assert_eq!(
            transform_text("thuowngf xuyeen luyeenj taapj", &cfg),
            "thường xuyên luyện tập"
        );
    }

    #[test]
    fn test_modern_vs_traditional_tone_style() {
        let modern = telex_modern();
        let trad = telex_traditional();

        assert_eq!(transform_text("hoaf hoef thuys tuyf", &modern), "hoà hoè thuý tuỳ");
        assert_eq!(transform_text("hoaf hoef thuys tuyf", &trad), "hòa hòe thúy tùy");

        // When a final consonant is added ("hoàn", "toàn", "quỳnh"), both styles place tone on 2nd vowel
        assert_eq!(transform_text("hoafn toafn quyfnh", &modern), "hoàn toàn quỳnh");
        assert_eq!(transform_text("hoafn toafn quyfnh", &trad), "hoàn toàn quỳnh");
    }

    #[test]
    fn test_vni_input_method() {
        let cfg = vni_modern();
        assert_eq!(
            transform_text("Vie65t Nam d9a61t nu7o71c con ngu7o72i", &cfg),
            "Việt Nam đất nước con người"
        );
        assert_eq!(
            transform_text("ho2a ho2an thu1y", &cfg),
            "hoà hoàn thuý"
        );
    }

    #[test]
    fn test_double_tap_undo_escapes() {
        let cfg = telex_modern();
        assert_eq!(transform_text("daaud", &cfg), "đâu");
        assert_eq!(transform_text("ddd", &cfg), "dd");
        assert_eq!(transform_text("aaa", &cfg), "aa");
        assert_eq!(transform_text("eee", &cfg), "ee");
        assert_eq!(transform_text("ooo", &cfg), "oo");
        assert_eq!(transform_text("ww", &cfg), "w");
        assert_eq!(transform_text("ass", &cfg), "as");
    }

    #[test]
    fn test_w_horn_and_breve_cases() {
        let cfg = telex_modern();
        assert_eq!(transform_text("nhw twf chwa chaats", &cfg), "như từ chưa chất");
        assert_eq!(transform_text("muaw", &cfg), "mưa");
        assert_eq!(transform_text("quawng hoawcj", &cfg), "quăng hoặc");
        assert_eq!(transform_text("huowng nuowcs", &cfg), "hương nước");
    }

    #[test]
    fn test_english_auto_restore() {
        let cfg = telex_modern();
        let english_words = [
            "window",
            "windows",
            "wayland",
            "with",
            "water",
            "world",
            "work",
            "word",
            "write",
            "restore",
            "fedora",
            "firefox",
            "software",
            "support",
            "export",
            "import",
            "report",
            "port",
            "part",
            "sort",
            "short",
            "chart",
            "start",
            "smart",
            "soft",
            "shift",
            "text",
            "next",
            "first",
            "rust",
            "center",
            "city",
            "get",
            "need",
            "good",
            "food",
            "book",
            "look",
            "root",
            "boot",
            "keep",
            "meet",
            "dad",
            "did",
            "dead",
            "status",
            "struct",
            "string",
            "system",
            "terminal",
            "github",
        ];

        for word in english_words {
            assert_eq!(
                transform_text(word, &cfg),
                word,
                "Failed to preserve English word: {word}"
            );
        }
    }

    #[test]
    fn test_minimal_suffix_diff() {
        let cfg = telex_modern();
        let mut buf = WordBuffer::new();

        // v -> "v"
        let d1 = buf.push_char('v', &cfg);
        assert_eq!(d1.backspace_chars, 0);
        assert_eq!(d1.insert_text, "v");

        // i -> "vi"
        let d2 = buf.push_char('i', &cfg);
        assert_eq!(d2.backspace_chars, 0);
        assert_eq!(d2.insert_text, "i");

        // e -> "vie"
        let d3 = buf.push_char('e', &cfg);
        assert_eq!(d3.backspace_chars, 0);
        assert_eq!(d3.insert_text, "e");

        // e -> "viê" (replaces only 1 char 'e' with 'ê'!)
        let d4 = buf.push_char('e', &cfg);
        assert_eq!(d4.backspace_chars, 1);
        assert_eq!(d4.delete_bytes, 1);
        assert_eq!(d4.insert_text, "ê");

        // t -> "viêt"
        let d5 = buf.push_char('t', &cfg);
        assert_eq!(d5.backspace_chars, 0);
        assert_eq!(d5.insert_text, "t");

        // j -> "việt" (replaces 2 chars "êt" with "ệt")
        let d6 = buf.push_char('j', &cfg);
        assert_eq!(d6.backspace_chars, 2);
        assert_eq!(d6.delete_bytes, "êt".len());
        assert_eq!(d6.insert_text, "ệt");
    }
}
