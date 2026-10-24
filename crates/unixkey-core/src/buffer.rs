use serde::{Deserialize, Serialize};

use crate::spellcheck::is_valid_vietnamese;
use crate::syllable::{
    BaseVowel, InputMethod, Tone, ToneStyle, VowelChar, VowelMod, is_d_or_stroke_d, is_stroke_d,
    toggle_stroke_d,
};
use crate::tone::{
    current_syllable_tone, find_tone_position, reposition_tone, split_syllable_parts,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineConfig {
    pub input_method: InputMethod,
    pub tone_style: ToneStyle,
    pub spellcheck: bool,
    pub auto_restore_english: bool,
    pub w_at_word_start_as_u_horn: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            input_method: InputMethod::Telex,
            tone_style: ToneStyle::Modern,
            spellcheck: true,
            auto_restore_english: true,
            w_at_word_start_as_u_horn: true,
        }
    }
}

/// Minimal edit instruction needed to update the client text field from `old_str` to `new_str`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff {
    /// Number of Unicode scalar values (`char`s) to delete from the end of `old_str`.
    pub backspace_chars: usize,
    /// Number of UTF-8 bytes to delete from the end of `old_str` (for `delete_surrounding_text`).
    pub delete_bytes: usize,
    /// New UTF-8 suffix string to insert after deleting the old suffix.
    pub insert_text: String,
}

impl Diff {
    pub fn compute(old_str: &str, new_str: &str) -> Self {
        let mut common_bytes = 0;
        let mut common_chars = 0;

        for (c1, c2) in old_str.chars().zip(new_str.chars()) {
            if c1 == c2 {
                common_bytes += c1.len_utf8();
                common_chars += 1;
            } else {
                break;
            }
        }

        let total_old_chars = old_str.chars().count();
        let backspace_chars = total_old_chars.saturating_sub(common_chars);
        let delete_bytes = old_str.len().saturating_sub(common_bytes);
        let insert_text = new_str[common_bytes..].to_string();

        Self {
            backspace_chars,
            delete_bytes,
            insert_text,
        }
    }

    pub fn is_noop(&self) -> bool {
        self.backspace_chars == 0 && self.insert_text.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct WordBuffer {
    raw_keys: Vec<char>,
    composed: Vec<char>,
    displayed: String,
    disabled_for_word: bool,
    transformed: bool,
}

impl WordBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.raw_keys.is_empty()
    }

    pub fn displayed(&self) -> &str {
        &self.displayed
    }

    pub fn raw_string(&self) -> String {
        self.raw_keys.iter().collect()
    }

    pub fn composed_string(&self) -> String {
        self.composed.iter().collect()
    }

    pub fn clear(&mut self) {
        self.raw_keys.clear();
        self.composed.clear();
        self.displayed.clear();
        self.disabled_for_word = false;
        self.transformed = false;
    }

    /// Processes a single printable character `c` and returns the `Diff` between the previous
    /// displayed word and the new displayed word.
    pub fn push_char(&mut self, c: char, cfg: &EngineConfig) -> Diff {
        let old_displayed = self.displayed.clone();
        self.raw_keys.push(c);

        if self.disabled_for_word {
            self.composed.push(c);
            self.displayed = self.composed_string();
            return Diff::compute(&old_displayed, &self.displayed);
        }

        let applied = match cfg.input_method {
            InputMethod::Telex => self.apply_telex_char(c, cfg),
            InputMethod::Vni => self.apply_vni_char(c, cfg),
        };

        if !applied {
            self.composed.push(c);
            if VowelChar::from_char(c).is_some() || c.is_alphabetic() {
                // If adding a vowel or final consonant shifts the canonical tone position
                // (e.g., "hoà" + 'n' -> "hoàn"), reposition the existing tone.
                if self.transformed {
                    reposition_tone(&mut self.composed, cfg.tone_style);
                }
            }
        }

        self.update_displayed_with_spellcheck(cfg, true);
        Diff::compute(&old_displayed, &self.displayed)
    }

    /// Handles Backspace within the active word buffer.
    /// Returns `Some(Diff)` if the buffer was non-empty and handled the backspace,
    /// or `None` if the buffer was already empty (caller should pass Backspace through).
    pub fn pop_char(&mut self, cfg: &EngineConfig) -> Option<Diff> {
        if self.raw_keys.is_empty() {
            return None;
        }
        let old_displayed = self.displayed.clone();

        if self.disabled_for_word {
            self.raw_keys.pop();
            self.composed.pop();
            if self.composed.is_empty() {
                self.clear();
            } else {
                self.displayed = self.composed_string();
            }
            return Some(Diff::compute(&old_displayed, &self.displayed));
        }

        // Remove the last visible character from `composed`.
        self.composed.pop();
        self.raw_keys.pop();

        if self.composed.is_empty() {
            self.clear();
            return Some(Diff::compute(&old_displayed, ""));
        }

        // Rebuild raw_keys to match composed so subsequent edits remain consistent,
        // and reposition tone if a final consonant was removed.
        reposition_tone(&mut self.composed, cfg.tone_style);
        self.displayed = self.composed_string();
        Some(Diff::compute(&old_displayed, &self.displayed))
    }

    /// Finalizes the word when a boundary character (Space, punctuation, Enter) is typed.
    /// Runs complete-syllable spellcheck; if the word is not valid Vietnamese and
    /// `auto_restore_english` is enabled, returns the `Diff` needed to restore the raw English word
    /// before the caller commits the boundary character, then clears the buffer.
    pub fn finalize_word(&mut self, cfg: &EngineConfig) -> Diff {
        self.finalize_word_full(cfg).1
    }

    /// Like `finalize_word`, but also returns the final resolved word string (useful for Preedit mode).
    pub fn finalize_word_full(&mut self, cfg: &EngineConfig) -> (String, Diff) {
        if self.raw_keys.is_empty() {
            return (
                String::new(),
                Diff {
                    backspace_chars: 0,
                    delete_bytes: 0,
                    insert_text: String::new(),
                },
            );
        }

        let old_displayed = self.displayed.clone();
        self.update_displayed_with_spellcheck(cfg, false);
        let final_word = self.displayed.clone();
        let diff = Diff::compute(&old_displayed, &final_word);
        self.clear();
        (final_word, diff)
    }

    fn update_displayed_with_spellcheck(&mut self, cfg: &EngineConfig, allow_incomplete: bool) {
        if self.disabled_for_word || !self.transformed {
            self.displayed = self.composed_string();
            return;
        }

        if cfg.spellcheck && cfg.auto_restore_english {
            let valid = is_valid_vietnamese(&self.composed, allow_incomplete)
                && self.check_bare_w_start_validity(allow_incomplete)
                && !self.is_plain_english_st_cluster();
            if !valid {
                self.displayed = self.raw_string();
                return;
            }
        }

        self.displayed = self.composed_string();
    }

    /// Extra phonotactic guard when a word started with a bare 'w' -> 'ư' (no initial consonant):
    /// In Vietnamese, words starting with 'ư' (no initial consonant) can only be:
    /// - Open: "ư", "ưa", "ưu", "ươi", "ươu"
    /// - Closed with 'ư': ONLY "ức"/"ực" (coda "c") or "ưng"/"ửng"/"ứng"/"ừng" (coda "ng")
    /// - Closed with "ươ": "ước", "ươm", "ươn", "ương", "ướp", "ướt"
    /// Words like "win" ("ưin"), "with" ("ưith"), "want" ("ưant"), "water" ("ưat"), "web" ("ưeb")
    /// are immediately rejected as English!
    fn check_bare_w_start_validity(&self, allow_incomplete: bool) -> bool {
        let Some(&first_raw) = self.raw_keys.first() else {
            return true;
        };
        if !matches!(first_raw, 'w' | 'W') {
            return true;
        }
        let Some((v_start, v_end, total_len)) = split_syllable_parts(&self.composed) else {
            return false;
        };
        if v_start != 0 {
            return true;
        }

        let mut nucleus = String::new();
        for &c in &self.composed[v_start..v_end] {
            if let Some(vc) = VowelChar::from_char(c) {
                nucleus.push(vc.base_and_mod_lower_char());
            }
        }
        let coda: String = self.composed[v_end..total_len]
            .iter()
            .flat_map(|c| c.to_lowercase())
            .collect();

        match nucleus.as_str() {
            "ư" => {
                if coda.is_empty() {
                    true
                } else if allow_incomplete && coda == "n" {
                    // Could be typing "ưng"
                    true
                } else {
                    matches!(coda.as_str(), "c" | "ng")
                }
            }
            "ưa" | "ưu" | "ươi" | "ươu" => coda.is_empty(),
            "ươ" | "ưo" => {
                coda.is_empty() || matches!(coda.as_str(), "c" | "m" | "n" | "ng" | "p" | "t")
            }
            _ => false,
        }
    }

    /// Detects English `-st` words (like "rust", "test", "best", "rest", "list", "must", "host", "post", "cost", "last")
    /// where the raw keys end in `['s', 't']` and the syllable has NO Vietnamese vowel/consonant diacritics
    /// (`ă, â, ê, ô, ơ, ư, đ`). In Vietnamese, words like "rút", "lít", "mút", "hót" are typed "ruts", "lits", "muts", "hots".
    fn is_plain_english_st_cluster(&self) -> bool {
        let len = self.raw_keys.len();
        if len < 3 {
            return false;
        }
        let r_second_last = self.raw_keys[len - 2].to_ascii_lowercase();
        let r_last = self.raw_keys[len - 1].to_ascii_lowercase();
        if (r_second_last, r_last) != ('s', 't') {
            return false;
        }

        // Check if any character in `composed` has a Vietnamese diacritic (ă, â, ê, ô, ơ, ư, đ)
        for &c in &self.composed {
            if is_stroke_d(c) {
                return false;
            }
            if let Some(vc) = VowelChar::from_char(c)
                && vc.modifier != VowelMod::None
            {
                return false;
            }
        }

        true
    }

    fn apply_telex_char(&mut self, c: char, cfg: &EngineConfig) -> bool {
        let lower = c.to_ascii_lowercase();

        // 1. Handle 'd' / 'D' -> 'đ' / 'Đ'
        if lower == 'd' {
            // Case 1a: Adjacent "dd" or triple "ddd"
            if let Some(&last) = self.composed.last() {
                if is_stroke_d(last) {
                    // Triple "ddd" -> undo stroke back to 'd' + 'd' and disable for word
                    let idx = self.composed.len() - 1;
                    self.composed[idx] = toggle_stroke_d(last);
                    self.composed.push(c);
                    self.disabled_for_word = true;
                    return true;
                }
                if is_d_or_stroke_d(last) && self.composed.len() == 1 {
                    self.composed[0] = toggle_stroke_d(last);
                    self.transformed = true;
                    return true;
                }
            }
            // Case 1b: Delayed 'd' at end of syllable (e.g. "doongd" -> "đông", "dajd" -> "đạ")
            // Only trigger non-adjacent 'd' stroke when the syllable already has a Vietnamese modifier/tone
            // so English words like "dad", "did", "dead" stay untouched!
            if self.composed.len() >= 2 && self.transformed && is_d_or_stroke_d(self.composed[0]) {
                if is_stroke_d(self.composed[0]) {
                    self.composed[0] = toggle_stroke_d(self.composed[0]);
                    self.composed.push(c);
                    self.disabled_for_word = true;
                } else {
                    self.composed[0] = toggle_stroke_d(self.composed[0]);
                }
                return true;
            }
            return false;
        }

        // 2. Handle Circumflex ('a' -> 'â', 'e' -> 'ê', 'o' -> 'ô')
        if matches!(lower, 'a' | 'e' | 'o') {
            let target_base = match lower {
                'a' => BaseVowel::A,
                'e' => BaseVowel::E,
                'o' => BaseVowel::O,
                _ => unreachable!(),
            };
            if self.try_apply_circumflex(target_base, c, cfg) {
                return true;
            }
            return false;
        }

        // 3. Handle Horn / Breve ('w' / 'W')
        if lower == 'w' {
            if self.try_apply_w(c, cfg) {
                return true;
            }
            return false;
        }

        // 4. Handle Tone keys ('s', 'f', 'r', 'x', 'j', 'z')
        let tone_opt = match lower {
            's' => Some(Tone::Acute),
            'f' => Some(Tone::Grave),
            'r' => Some(Tone::Hook),
            'x' => Some(Tone::Tilde),
            'j' => Some(Tone::Dot),
            'z' => Some(Tone::None),
            _ => None,
        };

        if let Some(target_tone) = tone_opt {
            return self.try_apply_tone(target_tone, c, cfg);
        }

        false
    }

    fn apply_vni_char(&mut self, c: char, cfg: &EngineConfig) -> bool {
        match c {
            '1' => self.try_apply_tone(Tone::Acute, c, cfg),
            '2' => self.try_apply_tone(Tone::Grave, c, cfg),
            '3' => self.try_apply_tone(Tone::Hook, c, cfg),
            '4' => self.try_apply_tone(Tone::Tilde, c, cfg),
            '5' => self.try_apply_tone(Tone::Dot, c, cfg),
            '0' => self.try_apply_tone(Tone::None, c, cfg),
            '6' => self.try_apply_vni_circumflex(c, cfg),
            '7' => self.try_apply_horn_on_vowel(c, cfg),
            '8' => self.try_apply_breve_on_a(c, cfg),
            '9' => {
                if let Some(&first) = self.composed.first()
                    && is_d_or_stroke_d(first)
                {
                    if is_stroke_d(first) {
                        self.composed[0] = toggle_stroke_d(first);
                        self.composed.push(c);
                        self.disabled_for_word = true;
                    } else {
                        self.composed[0] = toggle_stroke_d(first);
                        self.transformed = true;
                    }
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    fn try_apply_circumflex(&mut self, target_base: BaseVowel, raw_c: char, cfg: &EngineConfig) -> bool {
        let Some((v_start, v_end, _)) = split_syllable_parts(&self.composed) else {
            return false;
        };

        // Find vowel in nucleus matching `target_base`
        for idx in (v_start..v_end).rev() {
            let Some(mut vc) = VowelChar::from_char(self.composed[idx]) else {
                continue;
            };
            if vc.base != target_base {
                continue;
            }

            if vc.modifier == VowelMod::Circumflex {
                // Double-tap undo (e.g., "aaa" -> "aa", "eee" -> "ee", "ooo" -> "oo")
                vc.modifier = VowelMod::None;
                self.composed[idx] = vc.to_char();
                self.composed.push(raw_c);
                reposition_tone(&mut self.composed, cfg.tone_style);
                self.disabled_for_word = true;
                return true;
            }

            vc.modifier = VowelMod::Circumflex;
            self.composed[idx] = vc.to_char();
            reposition_tone(&mut self.composed, cfg.tone_style);
            self.transformed = true;
            return true;
        }

        false
    }

    fn try_apply_vni_circumflex(&mut self, raw_c: char, cfg: &EngineConfig) -> bool {
        let Some((v_start, v_end, _)) = split_syllable_parts(&self.composed) else {
            return false;
        };

        for idx in (v_start..v_end).rev() {
            let Some(mut vc) = VowelChar::from_char(self.composed[idx]) else {
                continue;
            };
            if matches!(vc.base, BaseVowel::A | BaseVowel::E | BaseVowel::O) {
                if vc.modifier == VowelMod::Circumflex {
                    vc.modifier = VowelMod::None;
                    self.composed[idx] = vc.to_char();
                    self.composed.push(raw_c);
                    reposition_tone(&mut self.composed, cfg.tone_style);
                    self.disabled_for_word = true;
                    return true;
                }
                vc.modifier = VowelMod::Circumflex;
                self.composed[idx] = vc.to_char();
                reposition_tone(&mut self.composed, cfg.tone_style);
                self.transformed = true;
                return true;
            }
        }
        false
    }

    fn try_apply_breve_on_a(&mut self, raw_c: char, cfg: &EngineConfig) -> bool {
        let Some((v_start, v_end, _)) = split_syllable_parts(&self.composed) else {
            return false;
        };

        for idx in (v_start..v_end).rev() {
            let Some(mut vc) = VowelChar::from_char(self.composed[idx]) else {
                continue;
            };
            if vc.base == BaseVowel::A {
                if vc.modifier == VowelMod::Breve {
                    vc.modifier = VowelMod::None;
                    self.composed[idx] = vc.to_char();
                    self.composed.push(raw_c);
                    reposition_tone(&mut self.composed, cfg.tone_style);
                    self.disabled_for_word = true;
                    return true;
                }
                vc.modifier = VowelMod::Breve;
                self.composed[idx] = vc.to_char();
                reposition_tone(&mut self.composed, cfg.tone_style);
                self.transformed = true;
                return true;
            }
        }
        false
    }

    fn try_apply_horn_on_vowel(&mut self, raw_c: char, cfg: &EngineConfig) -> bool {
        let Some((v_start, v_end, _)) = split_syllable_parts(&self.composed) else {
            return false;
        };

        // Check for "uo" / "uô" / "ưo" / "uơ" cluster in nucleus -> "ươ"
        if v_end - v_start >= 2 {
            for idx in v_start..(v_end - 1) {
                let (Some(mut vc0), Some(mut vc1)) = (
                    VowelChar::from_char(self.composed[idx]),
                    VowelChar::from_char(self.composed[idx + 1]),
                ) else {
                    continue;
                };
                if vc0.base == BaseVowel::U && vc1.base == BaseVowel::O {
                    if vc0.modifier == VowelMod::Horn && vc1.modifier == VowelMod::Horn {
                        // Undo "ươ" + w/7 -> "uo" + raw_c
                        vc0.modifier = VowelMod::None;
                        vc1.modifier = VowelMod::None;
                        self.composed[idx] = vc0.to_char();
                        self.composed[idx + 1] = vc1.to_char();
                        self.composed.push(raw_c);
                        reposition_tone(&mut self.composed, cfg.tone_style);
                        self.disabled_for_word = true;
                        return true;
                    }
                    vc0.modifier = VowelMod::Horn;
                    vc1.modifier = VowelMod::Horn;
                    self.composed[idx] = vc0.to_char();
                    self.composed[idx + 1] = vc1.to_char();
                    reposition_tone(&mut self.composed, cfg.tone_style);
                    self.transformed = true;
                    return true;
                }
            }
        }

        // Check for "ua" where 'u' is in nucleus (not "qu") -> "ưa" (e.g. "muaw" -> "mưa")
        if v_end - v_start == 2 {
            let (Some(mut vc0), Some(vc1)) = (
                VowelChar::from_char(self.composed[v_start]),
                VowelChar::from_char(self.composed[v_start + 1]),
            ) else {
                return false;
            };
            if vc0.base == BaseVowel::U && vc1.base == BaseVowel::A && vc1.modifier == VowelMod::None {
                if vc0.modifier == VowelMod::Horn {
                    vc0.modifier = VowelMod::None;
                    self.composed[v_start] = vc0.to_char();
                    self.composed.push(raw_c);
                    reposition_tone(&mut self.composed, cfg.tone_style);
                    self.disabled_for_word = true;
                    return true;
                }
                vc0.modifier = VowelMod::Horn;
                self.composed[v_start] = vc0.to_char();
                reposition_tone(&mut self.composed, cfg.tone_style);
                self.transformed = true;
                return true;
            }
        }

        // Single 'o' or 'u' in nucleus -> 'ơ' or 'ư'
        for idx in (v_start..v_end).rev() {
            let Some(mut vc) = VowelChar::from_char(self.composed[idx]) else {
                continue;
            };
            if matches!(vc.base, BaseVowel::O | BaseVowel::U) {
                // Do not turn medial 'o' in "oa" or "oe" into 'ơ' (in Telex, "oaw" modifies 'a' -> "oă")
                if vc.base == BaseVowel::O && idx + 1 < v_end {
                    if let Some(next_vc) = VowelChar::from_char(self.composed[idx + 1])
                        && matches!(next_vc.base, BaseVowel::A | BaseVowel::E)
                    {
                        continue;
                    }
                }
                if vc.modifier == VowelMod::Horn {
                    // If this 'ư' came from a standalone 'w' at word/consonant start,
                    // undo 'ư' directly to literal 'w' instead of "uw"!
                    let prev_raw_was_w = self
                        .raw_keys
                        .len()
                        .checked_sub(2)
                        .and_then(|i| self.raw_keys.get(i))
                        .is_some_and(|&r| matches!(r, 'w' | 'W'));

                    if prev_raw_was_w && vc.base == BaseVowel::U {
                        self.composed[idx] = if vc.uppercase { 'W' } else { 'w' };
                    } else {
                        vc.modifier = VowelMod::None;
                        self.composed[idx] = vc.to_char();
                        self.composed.push(raw_c);
                    }
                    reposition_tone(&mut self.composed, cfg.tone_style);
                    self.disabled_for_word = true;
                    return true;
                }
                vc.modifier = VowelMod::Horn;
                self.composed[idx] = vc.to_char();
                reposition_tone(&mut self.composed, cfg.tone_style);
                self.transformed = true;
                return true;
            }
        }

        false
    }

    fn try_apply_w(&mut self, raw_c: char, cfg: &EngineConfig) -> bool {
        // 1. If there are vowels, try horn ('uo' -> 'ươ', 'ua' -> 'ưa', 'o' -> 'ơ', 'u' -> 'ư')
        //    or breve ('a' -> 'ă', including 'oa' -> 'oă' and 'qua' -> 'quă').
        if split_syllable_parts(&self.composed).is_some() {
            // Note: in "oa" or "qua", 'a' is the target (becomes "oă" / "quă").
            // In "mua", 'u' is the target (becomes "mưa").
            // So we check horn first (which skips medial 'o' in "oa" and handles "mua" -> "mưa"),
            // then breve on 'a'!
            if self.try_apply_horn_on_vowel(raw_c, cfg) {
                return true;
            }
            if self.try_apply_breve_on_a(raw_c, cfg) {
                return true;
            }
            return false;
        }

        // 2. No vowels in `composed` yet (or only initial "q" where 'u' hasn't been typed):
        // If `w_at_word_start_as_u_horn` is enabled, standalone 'w' produces 'ư' / 'Ư'
        // as long as the consonant prefix is a valid Vietnamese initial (not 'q').
        if cfg.w_at_word_start_as_u_horn {
            let init_lower: String = self
                .composed
                .iter()
                .flat_map(|c| c.to_lowercase())
                .collect();
            if init_lower != "q" && is_valid_vietnamese(&self.composed, true) {
                let u_horn = if raw_c.is_uppercase() { 'Ư' } else { 'ư' };
                self.composed.push(u_horn);
                self.transformed = true;
                return true;
            }
        }

        false
    }

    fn try_apply_tone(&mut self, target_tone: Tone, raw_c: char, cfg: &EngineConfig) -> bool {
        let Some(pos) = find_tone_position(&self.composed, cfg.tone_style) else {
            return false;
        };

        let cur_tone = current_syllable_tone(&self.composed);

        if target_tone == Tone::None {
            // 'z' or '0' removes an existing tone. If there is no tone, treat 'z'/'0' as a literal character.
            if cur_tone == Tone::None {
                return false;
            }
            for c in self.composed.iter_mut() {
                if let Some(mut vc) = VowelChar::from_char(*c)
                    && vc.tone != Tone::None
                {
                    vc.tone = Tone::None;
                    *c = vc.to_char();
                }
            }
            self.transformed = true;
            return true;
        }

        if cur_tone == target_tone {
            // Double-tap tone key (e.g., "ss", "ff", "rr", "xx", "jj") -> undo tone and append literal key!
            for c in self.composed.iter_mut() {
                if let Some(mut vc) = VowelChar::from_char(*c)
                    && vc.tone != Tone::None
                {
                    vc.tone = Tone::None;
                    *c = vc.to_char();
                }
            }
            self.composed.push(raw_c);
            self.disabled_for_word = true;
            return true;
        }

        // Clear any existing tone on other vowels first
        for c in self.composed.iter_mut() {
            if let Some(mut vc) = VowelChar::from_char(*c)
                && vc.tone != Tone::None
            {
                vc.tone = Tone::None;
                *c = vc.to_char();
            }
        }

        if let Some(mut vc) = VowelChar::from_char(self.composed[pos]) {
            vc.tone = target_tone;
            self.composed[pos] = vc.to_char();
            self.transformed = true;
            return true;
        }

        false
    }
}
