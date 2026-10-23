use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum InputMethod {
    #[default]
    Telex,
    Vni,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ToneStyle {
    /// Modern orthography: hoà, hoè, thuý, tuỳ
    #[default]
    Modern,
    /// Traditional orthography: hòa, hòe, thúy, tùy
    Traditional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Tone {
    #[default]
    None,
    /// Sắc (/)
    Acute,
    /// Huyền (\)
    Grave,
    /// Hỏi (?)
    Hook,
    /// Ngã (~)
    Tilde,
    /// Nặng (.)
    Dot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum VowelMod {
    #[default]
    None,
    /// ^ (â, ê, ô)
    Circumflex,
    /// ă
    Breve,
    /// ơ, ư
    Horn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BaseVowel {
    A,
    E,
    I,
    O,
    U,
    Y,
}

impl BaseVowel {
    pub fn to_ascii_char(self, uppercase: bool) -> char {
        let c = match self {
            BaseVowel::A => 'a',
            BaseVowel::E => 'e',
            BaseVowel::I => 'i',
            BaseVowel::O => 'o',
            BaseVowel::U => 'u',
            BaseVowel::Y => 'y',
        };
        if uppercase { c.to_ascii_uppercase() } else { c }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VowelChar {
    pub base: BaseVowel,
    pub modifier: VowelMod,
    pub tone: Tone,
    pub uppercase: bool,
}

impl VowelChar {
    pub fn to_char(self) -> char {
        use BaseVowel::*;
        use Tone::*;
        use VowelMod::*;

        let lower = match (self.base, self.modifier, self.tone) {
            // a
            (A, VowelMod::None, Tone::None) => 'a',
            (A, VowelMod::None, Acute) => 'á',
            (A, VowelMod::None, Grave) => 'à',
            (A, VowelMod::None, Hook) => 'ả',
            (A, VowelMod::None, Tilde) => 'ã',
            (A, VowelMod::None, Dot) => 'ạ',
            // ă
            (A, Breve, Tone::None) => 'ă',
            (A, Breve, Acute) => 'ắ',
            (A, Breve, Grave) => 'ằ',
            (A, Breve, Hook) => 'ẳ',
            (A, Breve, Tilde) => 'ẵ',
            (A, Breve, Dot) => 'ặ',
            // â
            (A, Circumflex, Tone::None) => 'â',
            (A, Circumflex, Acute) => 'ấ',
            (A, Circumflex, Grave) => 'ầ',
            (A, Circumflex, Hook) => 'ẩ',
            (A, Circumflex, Tilde) => 'ẫ',
            (A, Circumflex, Dot) => 'ậ',
            // e
            (E, VowelMod::None, Tone::None) => 'e',
            (E, VowelMod::None, Acute) => 'é',
            (E, VowelMod::None, Grave) => 'è',
            (E, VowelMod::None, Hook) => 'ẻ',
            (E, VowelMod::None, Tilde) => 'ẽ',
            (E, VowelMod::None, Dot) => 'ẹ',
            // ê
            (E, Circumflex, Tone::None) => 'ê',
            (E, Circumflex, Acute) => 'ế',
            (E, Circumflex, Grave) => 'ề',
            (E, Circumflex, Hook) => 'ể',
            (E, Circumflex, Tilde) => 'ễ',
            (E, Circumflex, Dot) => 'ệ',
            // i
            (I, _, Tone::None) => 'i',
            (I, _, Acute) => 'í',
            (I, _, Grave) => 'ì',
            (I, _, Hook) => 'ỉ',
            (I, _, Tilde) => 'ĩ',
            (I, _, Dot) => 'ị',
            // o
            (O, VowelMod::None, Tone::None) => 'o',
            (O, VowelMod::None, Acute) => 'ó',
            (O, VowelMod::None, Grave) => 'ò',
            (O, VowelMod::None, Hook) => 'ỏ',
            (O, VowelMod::None, Tilde) => 'õ',
            (O, VowelMod::None, Dot) => 'ọ',
            // ô
            (O, Circumflex, Tone::None) => 'ô',
            (O, Circumflex, Acute) => 'ố',
            (O, Circumflex, Grave) => 'ồ',
            (O, Circumflex, Hook) => 'ổ',
            (O, Circumflex, Tilde) => 'ỗ',
            (O, Circumflex, Dot) => 'ộ',
            // ơ
            (O, Horn, Tone::None) => 'ơ',
            (O, Horn, Acute) => 'ớ',
            (O, Horn, Grave) => 'ờ',
            (O, Horn, Hook) => 'ở',
            (O, Horn, Tilde) => 'ỡ',
            (O, Horn, Dot) => 'ợ',
            // u
            (U, VowelMod::None, Tone::None) => 'u',
            (U, VowelMod::None, Acute) => 'ú',
            (U, VowelMod::None, Grave) => 'ù',
            (U, VowelMod::None, Hook) => 'ủ',
            (U, VowelMod::None, Tilde) => 'ũ',
            (U, VowelMod::None, Dot) => 'ụ',
            // ư
            (U, Horn, Tone::None) => 'ư',
            (U, Horn, Acute) => 'ứ',
            (U, Horn, Grave) => 'ừ',
            (U, Horn, Hook) => 'ử',
            (U, Horn, Tilde) => 'ữ',
            (U, Horn, Dot) => 'ự',
            // y
            (Y, _, Tone::None) => 'y',
            (Y, _, Acute) => 'ý',
            (Y, _, Grave) => 'ỳ',
            (Y, _, Hook) => 'ỷ',
            (Y, _, Tilde) => 'ỹ',
            (Y, _, Dot) => 'ỵ',
            // Invalid modifier fallback
            (b, _, t) => VowelChar {
                base: b,
                modifier: VowelMod::None,
                tone: t,
                uppercase: false,
            }
            .to_char(),
        };

        if self.uppercase {
            lower.to_uppercase().next().unwrap_or(lower)
        } else {
            lower
        }
    }

    pub fn from_char(c: char) -> Option<Self> {
        use BaseVowel::*;
        use Tone::*;
        use VowelMod::*;

        let uppercase = c.is_uppercase();
        let lower = c.to_lowercase().next()?;

        let (base, modifier, tone) = match lower {
            'a' => (A, VowelMod::None, Tone::None),
            'á' => (A, VowelMod::None, Acute),
            'à' => (A, VowelMod::None, Grave),
            'ả' => (A, VowelMod::None, Hook),
            'ã' => (A, VowelMod::None, Tilde),
            'ạ' => (A, VowelMod::None, Dot),

            'ă' => (A, Breve, Tone::None),
            'ắ' => (A, Breve, Acute),
            'ằ' => (A, Breve, Grave),
            'ẳ' => (A, Breve, Hook),
            'ẵ' => (A, Breve, Tilde),
            'ặ' => (A, Breve, Dot),

            'â' => (A, Circumflex, Tone::None),
            'ấ' => (A, Circumflex, Acute),
            'ầ' => (A, Circumflex, Grave),
            'ẩ' => (A, Circumflex, Hook),
            'ẫ' => (A, Circumflex, Tilde),
            'ậ' => (A, Circumflex, Dot),

            'e' => (E, VowelMod::None, Tone::None),
            'é' => (E, VowelMod::None, Acute),
            'è' => (E, VowelMod::None, Grave),
            'ẻ' => (E, VowelMod::None, Hook),
            'ẽ' => (E, VowelMod::None, Tilde),
            'ẹ' => (E, VowelMod::None, Dot),

            'ê' => (E, Circumflex, Tone::None),
            'ế' => (E, Circumflex, Acute),
            'ề' => (E, Circumflex, Grave),
            'ể' => (E, Circumflex, Hook),
            'ễ' => (E, Circumflex, Tilde),
            'ệ' => (E, Circumflex, Dot),

            'i' => (I, VowelMod::None, Tone::None),
            'í' => (I, VowelMod::None, Acute),
            'ì' => (I, VowelMod::None, Grave),
            'ỉ' => (I, VowelMod::None, Hook),
            'ĩ' => (I, VowelMod::None, Tilde),
            'ị' => (I, VowelMod::None, Dot),

            'o' => (O, VowelMod::None, Tone::None),
            'ó' => (O, VowelMod::None, Acute),
            'ò' => (O, VowelMod::None, Grave),
            'ỏ' => (O, VowelMod::None, Hook),
            'õ' => (O, VowelMod::None, Tilde),
            'ọ' => (O, VowelMod::None, Dot),

            'ô' => (O, Circumflex, Tone::None),
            'ố' => (O, Circumflex, Acute),
            'ồ' => (O, Circumflex, Grave),
            'ổ' => (O, Circumflex, Hook),
            'ỗ' => (O, Circumflex, Tilde),
            'ộ' => (O, Circumflex, Dot),

            'ơ' => (O, Horn, Tone::None),
            'ớ' => (O, Horn, Acute),
            'ờ' => (O, Horn, Grave),
            'ở' => (O, Horn, Hook),
            'ỡ' => (O, Horn, Tilde),
            'ợ' => (O, Horn, Dot),

            'u' => (U, VowelMod::None, Tone::None),
            'ú' => (U, VowelMod::None, Acute),
            'ù' => (U, VowelMod::None, Grave),
            'ủ' => (U, VowelMod::None, Hook),
            'ũ' => (U, VowelMod::None, Tilde),
            'ụ' => (U, VowelMod::None, Dot),

            'ư' => (U, Horn, Tone::None),
            'ứ' => (U, Horn, Acute),
            'ừ' => (U, Horn, Grave),
            'ử' => (U, Horn, Hook),
            'ữ' => (U, Horn, Tilde),
            'ự' => (U, Horn, Dot),

            'y' => (Y, VowelMod::None, Tone::None),
            'ý' => (Y, VowelMod::None, Acute),
            'ỳ' => (Y, VowelMod::None, Grave),
            'ỷ' => (Y, VowelMod::None, Hook),
            'ỹ' => (Y, VowelMod::None, Tilde),
            'ỵ' => (Y, VowelMod::None, Dot),

            _ => return Option::None,
        };

        Some(Self {
            base,
            modifier,
            tone,
            uppercase,
        })
    }

    pub fn base_and_mod_lower_char(self) -> char {
        Self {
            base: self.base,
            modifier: self.modifier,
            tone: Tone::None,
            uppercase: false,
        }
        .to_char()
    }
}

pub fn is_vowel(c: char) -> bool {
    VowelChar::from_char(c).is_some()
}

pub fn is_d_or_stroke_d(c: char) -> bool {
    matches!(c, 'd' | 'D' | 'đ' | 'Đ')
}

pub fn is_stroke_d(c: char) -> bool {
    matches!(c, 'đ' | 'Đ')
}

pub fn toggle_stroke_d(c: char) -> char {
    match c {
        'd' => 'đ',
        'D' => 'Đ',
        'đ' => 'd',
        'Đ' => 'D',
        other => other,
    }
}
