use crate::syllable::{BaseVowel, Tone, ToneStyle, VowelChar, VowelMod};

/// Splits a word into `(initial_range, vowel_range, coda_range)` indices in `chars`.
/// Returns `None` if there is no vowel or if consonants appear after vowels and then vowels again.
pub fn split_syllable_parts(chars: &[char]) -> Option<(usize, usize, usize)> {
    if chars.is_empty() {
        return None;
    }

    // Find first vowel
    let mut v_start = chars.iter().position(|&c| VowelChar::from_char(c).is_some())?;

    // Find end of contiguous vowel block
    let mut v_end = v_start;
    while v_end < chars.len() && VowelChar::from_char(chars[v_end]).is_some() {
        v_end += 1;
    }

    // Check if any vowel appears in the coda (which makes it multi-syllabic / non-Vietnamese)
    for &c in &chars[v_end..] {
        if VowelChar::from_char(c).is_some() {
            return None;
        }
    }

    // Handle "qu" and "gi" initial clusters:
    // If initial + first vowel is "qu" + another vowel, 'u' belongs to initial "qu".
    if v_start == 1 && v_end > v_start + 1 {
        let c0 = chars[0].to_ascii_lowercase();
        if let Some(vc) = VowelChar::from_char(chars[1]) {
            if c0 == 'q' && vc.base == BaseVowel::U && vc.modifier == VowelMod::None {
                v_start += 1;
            } else if c0 == 'g' && vc.base == BaseVowel::I && vc.modifier == VowelMod::None {
                v_start += 1;
            }
        }
    }

    Some((v_start, v_end, chars.len()))
}

/// Determines which character index in `chars` should hold the tone mark.
pub fn find_tone_position(chars: &[char], style: ToneStyle) -> Option<usize> {
    let (v_start, v_end, total_len) = split_syllable_parts(chars)?;
    let v_len = v_end - v_start;
    let has_coda = v_end < total_len;

    if v_len == 0 {
        return None;
    }
    if v_len == 1 {
        return Some(v_start);
    }

    let v0 = VowelChar::from_char(chars[v_start])?;
    let v1 = VowelChar::from_char(chars[v_start + 1])?;

    if v_len == 2 {
        if has_coda {
            // Closed diphthong (e.g., toán, tuần, việt, muốn, mượn, quỳnh): always second vowel
            return Some(v_start + 1);
        }

        // Open diphthong (no final consonant):
        // Special case: "ưa" -> first vowel 'ư' gets the tone (mưa, chữa, thừa)
        if v0.base == BaseVowel::U
            && v0.modifier == VowelMod::Horn
            && v1.base == BaseVowel::A
            && v1.modifier == VowelMod::None
        {
            return Some(v_start);
        }

        // Special case: "ươ" -> second vowel 'ơ' gets the tone
        if v0.base == BaseVowel::U
            && v0.modifier == VowelMod::Horn
            && v1.base == BaseVowel::O
            && v1.modifier == VowelMod::Horn
        {
            return Some(v_start + 1);
        }

        // If second vowel has a diacritic (iê, yê, uô, uâ, uê, uơ), tone on second vowel
        if v1.modifier != VowelMod::None {
            return Some(v_start + 1);
        }

        // If first vowel has a diacritic (âu, ây, êu, ôi, ơi, ưi, ưu), tone on first vowel
        if v0.modifier != VowelMod::None {
            return Some(v_start);
        }

        // Neither vowel has a diacritic: check oa, oe, uy
        let is_oa_oe_uy = matches!(
            (v0.base, v1.base),
            (BaseVowel::O, BaseVowel::A)
                | (BaseVowel::O, BaseVowel::E)
                | (BaseVowel::U, BaseVowel::Y)
        );

        if is_oa_oe_uy {
            return match style {
                ToneStyle::Modern => Some(v_start + 1),
                ToneStyle::Traditional => Some(v_start),
            };
        }

        // All other open diphthongs (ai, ao, au, ay, eo, ia, iu, oi, ua, ui): first vowel
        return Some(v_start);
    }

    // 3 or more vowels (triphthongs: iêu, yêu, oai, oay, oeo, uây, uôi, ươi, ươu, uya, uyê, uyu)
    let v2 = VowelChar::from_char(chars[v_start + 2])?;
    if v0.base == BaseVowel::U && v1.base == BaseVowel::Y && v2.base == BaseVowel::E {
        // uyê (e.g., chuyện, tuyết, khuyên): tone goes on the 3rd vowel ('ê')
        return Some(v_start + 2);
    }

    // Standard triphthong: tone on the middle vowel
    Some(v_start + 1)
}

/// Extracts any existing tone from `chars` and places it on the canonical vowel position.
pub fn reposition_tone(chars: &mut [char], style: ToneStyle) {
    let mut current_tone = Tone::None;
    for c in chars.iter_mut() {
        if let Some(mut vc) = VowelChar::from_char(*c)
            && vc.tone != Tone::None
        {
            current_tone = vc.tone;
            vc.tone = Tone::None;
            *c = vc.to_char();
        }
    }

    if current_tone != Tone::None
        && let Some(pos) = find_tone_position(chars, style)
        && let Some(mut vc) = VowelChar::from_char(chars[pos])
    {
        vc.tone = current_tone;
        chars[pos] = vc.to_char();
    }
}

/// Returns the current tone of the syllable in `chars`.
pub fn current_syllable_tone(chars: &[char]) -> Tone {
    for &c in chars {
        if let Some(vc) = VowelChar::from_char(c)
            && vc.tone != Tone::None
        {
            return vc.tone;
        }
    }
    Tone::None
}
