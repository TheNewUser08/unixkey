use crate::syllable::{Tone, VowelChar};
use crate::tone::{current_syllable_tone, split_syllable_parts};

const VALID_INITIALS: &[&str] = &[
    "", "b", "c", "ch", "d", "đ", "g", "gh", "gi", "h", "k", "kh", "l", "m", "n", "ng", "ngh",
    "nh", "p", "ph", "qu", "r", "s", "t", "th", "tr", "v", "x",
];

/// Prefixes of valid initials while the user hasn't typed a vowel yet (includes "q").
const VALID_INITIAL_PREFIXES: &[&str] = &[
    "", "b", "c", "ch", "d", "đ", "g", "gh", "gi", "h", "k", "kh", "l", "m", "n", "ng", "ngh",
    "nh", "p", "ph", "q", "qu", "r", "s", "t", "th", "tr", "v", "x",
];

const VALID_CODAS: &[&str] = &["", "c", "ch", "m", "n", "ng", "nh", "p", "t"];

/// Vowel nuclei that are valid in open syllables (no final consonant) or while typing.
const VALID_OPEN_NUCLEI: &[&str] = &[
    // Single vowels
    "a", "ă", "â", "e", "ê", "i", "o", "ô", "ơ", "u", "ư", "y",
    // Open diphthongs
    "ai", "ao", "au", "ay", "âu", "ây", "eo", "êu", "ia", "iê", "ie", "iu", "oa", "oă", "oe",
    "oi", "ôi", "ơi", "oo", "ua", "uă", "uâ", "uê", "ue", "ui", "uô", "uo", "uơ", "uy", "ưa",
    "ưi", "ươ", "ưo", "uơ", "ưu", "yê", "ye",
    // Triphthongs (including intermediate unmodified forms before second modifier key)
    "iêu", "ieu", "yêu", "yeu", "oai", "oay", "oeo", "uây", "uay", "uôi", "uoi", "ươi", "ưoi",
    "uơi", "ươu", "uou", "uya", "uyê", "uye", "uyu",
];

/// Vowel nuclei that can precede a final consonant (coda: c, ch, m, n, ng, nh, p, t).
/// Closing diphthongs ending in -i/-y/-u/-o (ai, ao, au, ay, eo, iu, oi, ui, ưi, ưu, etc.)
/// NEVER take a final consonant in Vietnamese.
const VALID_CLOSED_NUCLEI: &[&str] = &[
    // Single vowels
    "a", "ă", "â", "e", "ê", "i", "o", "ô", "ơ", "u", "ư", "y",
    // Medial + nucleus diphthongs that accept codas (including intermediate unmodified forms)
    "iê", "ie", "oa", "oă", "oe", "oo", "uâ", "ua", "uă", "uê", "ue", "uô", "uo", "uơ", "uy",
    "ươ", "ưo", "yê", "ye", // Only triphthong that accepts a coda (-n, -t) is uyê / uye
    "uyê", "uye",
];

/// Checks whether `chars` is a phonotactically valid Vietnamese syllable (or valid in-progress syllable).
pub fn is_valid_vietnamese(chars: &[char], allow_incomplete: bool) -> bool {
    if chars.is_empty() {
        return true;
    }

    // All characters must be alphabetic Vietnamese letters (no f, j, w, z in composed output)
    for &c in chars {
        let lower = c.to_lowercase().next().unwrap_or(c);
        if matches!(lower, 'f' | 'j' | 'w' | 'z') || !lower.is_alphabetic() {
            return false;
        }
    }

    // If there are no vowels yet, check if the consonant cluster is a valid initial prefix
    if chars.iter().all(|&c| VowelChar::from_char(c).is_none()) {
        if !allow_incomplete {
            return false;
        }
        let s: String = chars.iter().flat_map(|c| c.to_lowercase()).collect();
        return VALID_INITIAL_PREFIXES.contains(&s.as_str());
    }

    // Split into (v_start, v_end, total_len)
    let Some((v_start, v_end, total_len)) = split_syllable_parts(chars) else {
        // Contains vowel -> consonant -> vowel (multi-syllable English word like "restore", "fedora")
        return false;
    };

    let initial: String = chars[..v_start]
        .iter()
        .flat_map(|c| c.to_lowercase())
        .collect();
    if !VALID_INITIALS.contains(&initial.as_str()) {
        return false;
    }

    // Extract toneless lowercase vowel nucleus string
    let mut nucleus = String::with_capacity(v_end - v_start);
    for &c in &chars[v_start..v_end] {
        let Some(vc) = VowelChar::from_char(c) else {
            return false;
        };
        nucleus.push(vc.base_and_mod_lower_char());
    }

    let coda: String = chars[v_end..total_len]
        .iter()
        .flat_map(|c| c.to_lowercase())
        .collect();

    if !VALID_CODAS.contains(&coda.as_str()) {
        return false;
    }

    let first_vowel = nucleus.chars().next().unwrap_or(' ');
    let is_front_vowel = matches!(first_vowel, 'e' | 'ê' | 'i' | 'y');

    // Contextual orthographic rules for Vietnamese initials:
    match initial.as_str() {
        "c" if is_front_vowel => return false, // e.g., "center", "city", "cell"
        "k" if !is_front_vowel => return false,
        "gh" | "ngh" if !matches!(first_vowel, 'e' | 'ê' | 'i') => return false,
        "g" if matches!(first_vowel, 'e' | 'ê') => return false, // e.g., "get", "gem"
        "ng" if matches!(first_vowel, 'e' | 'ê' | 'i') => return false,
        _ => {}
    }

    if coda.is_empty() {
        if !VALID_OPEN_NUCLEI.contains(&nucleus.as_str()) {
            return false;
        }
        if !allow_incomplete {
            // Standalone ă, â or unmodified intermediate clusters (ie, uo, ye, uye) cannot end an open syllable
            if matches!(
                nucleus.as_str(),
                "ă" | "â" | "ie" | "uo" | "ưo" | "ye" | "uye" | "uay" | "uă" | "uâ" | "oă"
            ) {
                return false;
            }
        }
    } else {
        // Has a final consonant (coda)
        // Note: "ua" + coda is only valid when initial is "q"/"qu" (already split) or as intermediate for "uô"/"uâ" while typing
        if nucleus == "ua" && !allow_incomplete {
            return false;
        }
        if !VALID_CLOSED_NUCLEI.contains(&nucleus.as_str()) {
            return false;
        }
        if !allow_incomplete && matches!(nucleus.as_str(), "ie" | "uo" | "ưo" | "ye" | "uye") {
            return false;
        }

        // Stop codas (c, ch, p, t) ONLY permit Acute (Sắc) or Dot (Nặng) tones in Vietnamese!
        // Rejecting Grave (f), Hook (r), and Tilde (x) on stop codas catches English words like
        // "port", "part", "sort", "chart", "soft", "shift", "text", "next", "export", "support".
        let is_stop_coda = matches!(coda.as_str(), "c" | "ch" | "p" | "t");
        if is_stop_coda {
            let tone = current_syllable_tone(chars);
            if matches!(tone, Tone::Grave | Tone::Hook | Tone::Tilde) {
                return false;
            }
            if !allow_incomplete && tone == Tone::None {
                return false;
            }
        }
    }

    true
}
