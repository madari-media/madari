//! Display common ISO 639-1/2 codes without discarding unknown language tags.
pub fn name(code: &str) -> String {
    let normalized = code.trim().replace('_', "-").to_ascii_lowercase();
    let mut tags = normalized.split('-');
    let language = tags.next().unwrap_or_default();
    let name = match language {
        "en" | "eng" => "English",
        "fr" | "fra" | "fre" => "French",
        "de" | "deu" | "ger" => "German",
        "es" | "spa" => "Spanish",
        "pt" | "por" => "Portuguese",
        "it" | "ita" => "Italian",
        "ru" | "rus" => "Russian",
        "uk" | "ukr" => "Ukrainian",
        "ja" | "jpn" => "Japanese",
        "ko" | "kor" => "Korean",
        "zh" | "zho" | "chi" => "Chinese",
        "ar" | "ara" => "Arabic",
        "hi" | "hin" => "Hindi",
        "mr" | "mar" => "Marathi",
        "ta" | "tam" => "Tamil",
        "te" | "tel" => "Telugu",
        "ml" | "mal" => "Malayalam",
        "kn" | "kan" => "Kannada",
        "bn" | "ben" => "Bengali",
        "gu" | "guj" => "Gujarati",
        "pa" | "pan" => "Punjabi",
        "ur" | "urd" => "Urdu",
        "or" | "ori" => "Odia",
        "as" | "asm" => "Assamese",
        "ne" | "nep" => "Nepali",
        "si" | "sin" => "Sinhala",
        "th" | "tha" => "Thai",
        "vi" | "vie" => "Vietnamese",
        "id" | "ind" => "Indonesian",
        "ms" | "msa" | "may" => "Malay",
        "tl" | "tgl" | "fil" => "Filipino",
        "tr" | "tur" => "Turkish",
        "fa" | "fas" | "per" => "Persian",
        "he" | "heb" | "iw" => "Hebrew",
        "pl" | "pol" => "Polish",
        "nl" | "nld" | "dut" => "Dutch",
        "sv" | "swe" => "Swedish",
        "no" | "nor" => "Norwegian",
        "nb" | "nob" => "Norwegian Bokmål",
        "nn" | "nno" => "Norwegian Nynorsk",
        "da" | "dan" => "Danish",
        "fi" | "fin" => "Finnish",
        "el" | "ell" | "gre" => "Greek",
        "cs" | "ces" | "cze" => "Czech",
        "sk" | "slk" | "slo" => "Slovak",
        "hu" | "hun" => "Hungarian",
        "ro" | "ron" | "rum" => "Romanian",
        "bg" | "bul" => "Bulgarian",
        "hr" | "hrv" => "Croatian",
        "sr" | "srp" => "Serbian",
        "sl" | "slv" => "Slovenian",
        "bs" | "bos" => "Bosnian",
        "sq" | "sqi" | "alb" => "Albanian",
        "mk" | "mkd" | "mac" => "Macedonian",
        "et" | "est" => "Estonian",
        "lv" | "lav" => "Latvian",
        "lt" | "lit" => "Lithuanian",
        "is" | "isl" | "ice" => "Icelandic",
        "ca" | "cat" => "Catalan",
        "eu" | "eus" | "baq" => "Basque",
        "gl" | "glg" => "Galician",
        "ka" | "kat" | "geo" => "Georgian",
        "hy" | "hye" | "arm" => "Armenian",
        "az" | "aze" => "Azerbaijani",
        "kk" | "kaz" => "Kazakh",
        "uz" | "uzb" => "Uzbek",
        "sw" | "swa" => "Swahili",
        "af" | "afr" => "Afrikaans",
        "zu" | "zul" => "Zulu",
        "und" => "Unspecified language",
        "mul" => "Multiple languages",
        "zxx" => "No spoken language",
        "" => return String::new(),
        _ => return code.trim().to_string(),
    };
    let qualifiers: Vec<_> = tags
        .map(|tag| match tag {
            "us" => "United States".into(),
            "gb" => "United Kingdom".into(),
            "br" => "Brazil".into(),
            "pt" => "Portugal".into(),
            "ca" => "Canada".into(),
            "mx" => "Mexico".into(),
            "hans" => "Simplified".into(),
            "hant" => "Traditional".into(),
            "latn" => "Latin".into(),
            "cyrl" => "Cyrillic".into(),
            other => other.to_ascii_uppercase(),
        })
        .collect();
    if qualifiers.is_empty() {
        name.into()
    } else {
        format!("{name} ({})", qualifiers.join(", "))
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn displays_codes_and_preserves_unknowns() {
        for code in ["en", "EN", "eng"] {
            assert_eq!(super::name(code), "English");
        }
        assert_eq!(super::name("pt-BR"), "Portuguese (Brazil)");
        assert_eq!(super::name("hi"), "Hindi");
        assert_eq!(super::name("unknown-tag"), "unknown-tag");
    }
}
